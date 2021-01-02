use crate::unique_key::UniqueKey;
use crate::radio::{RadioParam, RadioParamValue, RadioError};
use std::collections::VecDeque;

struct RadioBackedParam<T> {
    param: RadioParam,
    current_value: T,
    updates: VecDeque<(T, UniqueKey)>, // This UniqueKey is used in the process-set-result
    updating: Option<UniqueKey?>,
}

struct RadioUpdateRequest {
    key: UniqueKey,
    param: RadioParam,
    value: RadioParamValue,
}

impl<T> AsRef<T> for RadioBackedParam<T> {
    fn as_ref(&self) -> &T {
        &self.current_value
    }
}

impl<T> RadioBackedParam<T> {
    pub fn new(param: RadioParam, current_value: T) -> Self {
        Self {
            param: param,
            value: initial,
            dirty: true,
            updating: None,
        }
    }

    pub fn set(&mut self, new_value: T) -> impl Future<Result<(), RadioError>> {
    }

    pub fn process_set_result(&mut self, key: UniqueKey, success: bool) {
        if let Some(current_key) == self.updating {
            if current_key == key {
                self.updating.take();
                if !success {
                    self.dirty = true;
                }
            }
        }
    }
}

impl<T : Into<RadioParamValue>>
