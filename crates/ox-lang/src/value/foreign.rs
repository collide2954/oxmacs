use std::any::Any;
use std::rc::Rc;
use std::sync::Arc;

use crate::symbol::Sym;

#[derive(Debug, Clone)]
pub(crate) enum Holder {
    Local(Rc<dyn Any>),
    Shared(Arc<dyn Any + Send + Sync>),
}

#[derive(Debug, Clone)]
pub(crate) struct Foreign {
    pub(crate) name: Sym,
    pub(crate) holder: Holder,
}

impl Foreign {
    pub(crate) fn get<T: Any>(&self) -> Option<&T> {
        match &self.holder {
            Holder::Local(value) => value.downcast_ref(),
            Holder::Shared(value) => value.downcast_ref(),
        }
    }

    pub(crate) fn address(&self) -> usize {
        match &self.holder {
            Holder::Local(value) => Rc::as_ptr(value).cast::<()>().addr(),
            Holder::Shared(value) => Arc::as_ptr(value).cast::<()>().addr(),
        }
    }
}
