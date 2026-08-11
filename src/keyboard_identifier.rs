use crate::interface::*;

pub fn get_keyboard_list<S: DeviceSource>(source: &S) -> Vec<S::Device> {
    source.get_keyboards().into_iter().collect()
}

pub fn drop_all_listeners() {}
