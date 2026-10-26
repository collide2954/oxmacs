use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use windows_sys::Win32::Foundation::{CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE};
use windows_sys::Win32::System::Diagnostics::Debug::{
    AddrModeFlat, CONTEXT, CONTEXT_FULL_AMD64, GetThreadContext, IMAGEHLP_LINEW64,
    ReadProcessMemory, STACKFRAME_EX, SYM_STKWALK_DEFAULT, SYMBOL_INFOW, SYMOPT_LOAD_LINES,
    SYMOPT_UNDNAME, StackWalkEx, SymCleanup, SymFromInlineContextW, SymFunctionTableAccess64,
    SymGetLineFromInlineContextW, SymGetModuleBase64, SymInitializeW, SymSetOptions,
};
use windows_sys::Win32::System::Memory::{MEMORY_BASIC_INFORMATION, VirtualQuery};
use windows_sys::Win32::System::SystemInformation::IMAGE_FILE_MACHINE_AMD64;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentThread, ResumeThread, SuspendThread,
};
use windows_sys::core::BOOL;

use crate::tables::Frame;

const EVERY: Duration = Duration::from_micros(500);
const DEPTH: usize = 64;
const STACK: usize = 256 * 1024;
const NAME: u32 = 512;

static SYMBOLS: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Address {
    pc: u64,
    inline: u32,
}

#[derive(Debug)]
struct Target {
    process: HANDLE,
    thread: HANDLE,
}

unsafe impl Send for Target {}

#[repr(C, align(16))]
struct Aligned(CONTEXT);

struct Snapshot {
    base: u64,
    bytes: Vec<u8>,
}

thread_local! {
    static SNAPSHOT: RefCell<Snapshot> = const {
        RefCell::new(Snapshot {
            base: 0,
            bytes: Vec::new(),
        })
    };
}

fn size<T>() -> u32 {
    u32::try_from(size_of::<T>()).unwrap_or(u32::MAX)
}

unsafe extern "system" fn read(
    process: HANDLE,
    address: u64,
    buffer: *mut c_void,
    wanted: u32,
    done: *mut u32,
) -> BOOL {
    let length = usize::try_from(wanted).unwrap_or(0);
    let copied = SNAPSHOT.with_borrow(|snapshot| {
        let start = usize::try_from(address.checked_sub(snapshot.base)?).ok()?;
        let bytes = snapshot.bytes.get(start..start.checked_add(length)?)?;
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.cast::<u8>(), length) };
        Some(())
    });
    if copied.is_some() {
        unsafe { *done = wanted };
        return 1;
    }
    let mut read = 0;
    let ok = unsafe {
        ReadProcessMemory(
            process,
            std::ptr::without_provenance(usize::try_from(address).unwrap_or(0)),
            buffer,
            length,
            &raw mut read,
        )
    };
    unsafe { *done = u32::try_from(read).unwrap_or(0) };
    ok
}

impl Target {
    fn capture(&self, context: &mut CONTEXT, copy: &mut [u8]) -> Option<(u64, usize)> {
        if unsafe { SuspendThread(self.thread) } == u32::MAX {
            return None;
        }
        let mut captured = None;
        if unsafe { GetThreadContext(self.thread, context) } != 0 {
            let top: *const c_void =
                std::ptr::without_provenance(usize::try_from(context.Rsp).unwrap_or(0));
            let mut region = MEMORY_BASIC_INFORMATION::default();
            let found = unsafe {
                VirtualQuery(top, &raw mut region, size_of::<MEMORY_BASIC_INFORMATION>())
            };
            let end = region.BaseAddress.addr().saturating_add(region.RegionSize);
            let length = end.saturating_sub(top.addr()).min(copy.len());
            let mut read = 0;
            if found != 0
                && unsafe {
                    ReadProcessMemory(
                        self.process,
                        top,
                        copy.as_mut_ptr().cast(),
                        length,
                        &raw mut read,
                    )
                } != 0
            {
                captured = Some((context.Rsp, read));
            }
        }
        unsafe { ResumeThread(self.thread) };
        captured
    }

    fn walk(&self, stack: &mut Vec<Address>, copy: &mut Vec<u8>) {
        stack.clear();
        let mut aligned = Aligned(CONTEXT {
            ContextFlags: CONTEXT_FULL_AMD64,
            ..CONTEXT::default()
        });
        let context = &mut aligned.0;
        let Some((base, length)) = self.capture(context, copy) else {
            return;
        };
        SNAPSHOT.with_borrow_mut(|snapshot| {
            snapshot.base = base;
            std::mem::swap(&mut snapshot.bytes, copy);
            snapshot.bytes.truncate(length);
        });
        let mut frame = STACKFRAME_EX {
            StackFrameSize: size::<STACKFRAME_EX>(),
            ..STACKFRAME_EX::default()
        };
        frame.AddrPC.Offset = context.Rip;
        frame.AddrPC.Mode = AddrModeFlat;
        frame.AddrFrame.Offset = context.Rbp;
        frame.AddrFrame.Mode = AddrModeFlat;
        frame.AddrStack.Offset = context.Rsp;
        frame.AddrStack.Mode = AddrModeFlat;
        while stack.len() < DEPTH
            && unsafe {
                StackWalkEx(
                    u32::from(IMAGE_FILE_MACHINE_AMD64),
                    self.process,
                    self.thread,
                    &raw mut frame,
                    (&raw mut *context).cast(),
                    Some(read),
                    Some(SymFunctionTableAccess64),
                    Some(SymGetModuleBase64),
                    None,
                    SYM_STKWALK_DEFAULT,
                )
            } != 0
            && frame.AddrPC.Offset != 0
        {
            stack.push(Address {
                pc: frame.AddrPC.Offset,
                inline: frame.InlineFrameContext,
            });
        }
        SNAPSHOT.with_borrow_mut(|snapshot| {
            std::mem::swap(&mut snapshot.bytes, copy);
            copy.resize(STACK, 0);
        });
    }
}

pub fn sampled(work: impl FnOnce() -> Result<(), String>) -> Result<Vec<Vec<Frame>>, String> {
    let _symbols = SYMBOLS.lock().unwrap_or_else(PoisonError::into_inner);
    let process = unsafe { GetCurrentProcess() };
    let folder: Vec<u16> = std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.parent()
                .map(|folder| folder.as_os_str().encode_wide().collect::<Vec<u16>>())
        })
        .unwrap_or_default()
        .into_iter()
        .chain([0])
        .collect();
    unsafe { SymSetOptions(SYMOPT_LOAD_LINES | SYMOPT_UNDNAME) };
    if unsafe { SymInitializeW(process, folder.as_ptr(), 1) } == 0 {
        return Err(format!(
            "Couldn't read symbols: {}",
            std::io::Error::last_os_error()
        ));
    }
    let mut current = std::ptr::null_mut();
    let duplicated = unsafe {
        DuplicateHandle(
            process,
            GetCurrentThread(),
            process,
            &raw mut current,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        )
    };
    if duplicated == 0 {
        unsafe { SymCleanup(process) };
        return Err(format!(
            "Couldn't watch this thread: {}",
            std::io::Error::last_os_error()
        ));
    }
    let target = Target {
        process,
        thread: current,
    };
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = Arc::clone(&stop);
    let sampler = thread::spawn(move || {
        let mut stacks = Vec::new();
        let mut stack = Vec::with_capacity(DEPTH);
        let mut copy = vec![0_u8; STACK];
        while !stopping.load(Ordering::Relaxed) {
            thread::sleep(EVERY);
            target.walk(&mut stack, &mut copy);
            if !stack.is_empty() {
                stacks.push(stack.clone());
            }
        }
        (stacks, target)
    });
    let worked = work();
    stop.store(true, Ordering::Relaxed);
    let (stacks, target) = sampler
        .join()
        .map_err(|_| "The sampler stopped unexpectedly".to_owned())?;
    let mut names: HashMap<Address, Frame> = HashMap::new();
    let resolved = stacks
        .iter()
        .map(|stack| {
            stack
                .iter()
                .map(|address| {
                    names
                        .entry(*address)
                        .or_insert_with(|| resolve(process, *address))
                        .clone()
                })
                .collect()
        })
        .collect();
    unsafe {
        CloseHandle(target.thread);
        SymCleanup(process);
    }
    worked.map(|()| resolved)
}

fn resolve(process: HANDLE, address: Address) -> Frame {
    Frame {
        function: function_name(process, address).unwrap_or_else(|| format!("{:#x}", address.pc)),
        place: place(process, address).unwrap_or_default(),
    }
}

fn function_name(process: HANDLE, address: Address) -> Option<String> {
    let bytes = size_of::<SYMBOL_INFOW>() + usize::try_from(NAME).ok()? * 2;
    let mut buffer = vec![0_u64; bytes.div_ceil(size_of::<u64>())];
    let symbol = buffer.as_mut_ptr().cast::<SYMBOL_INFOW>();
    let mut displacement = 0;
    unsafe {
        (*symbol).SizeOfStruct = size::<SYMBOL_INFOW>();
        (*symbol).MaxNameLen = NAME;
        if SymFromInlineContextW(
            process,
            address.pc,
            address.inline,
            &raw mut displacement,
            symbol,
        ) == 0
        {
            return None;
        }
        let length = usize::try_from((*symbol).NameLen.min(NAME)).ok()?;
        let name = std::slice::from_raw_parts((&raw const (*symbol).Name).cast::<u16>(), length);
        Some(String::from_utf16_lossy(name))
    }
}

fn place(process: HANDLE, address: Address) -> Option<String> {
    let mut line = IMAGEHLP_LINEW64 {
        SizeOfStruct: size::<IMAGEHLP_LINEW64>(),
        ..IMAGEHLP_LINEW64::default()
    };
    let mut offset = 0;
    let file = unsafe {
        if SymGetLineFromInlineContextW(
            process,
            address.pc,
            address.inline,
            0,
            &raw mut offset,
            &raw mut line,
        ) == 0
            || line.FileName.is_null()
        {
            return None;
        }
        let mut length = 0;
        while *line.FileName.add(length) != 0 {
            length += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(line.FileName, length))
    };
    let mut parts: Vec<&str> = file.rsplit(['\\', '/']).take(2).collect();
    parts.reverse();
    Some(format!("{}:{}", parts.join("/"), line.LineNumber))
}
