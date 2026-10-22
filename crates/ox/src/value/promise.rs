use crate::value::Value;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Promise {
    Pending { awaited: bool },
    Delivered(Value),
    Failed(Value),
}
