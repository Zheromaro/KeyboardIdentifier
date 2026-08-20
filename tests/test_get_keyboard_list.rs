mod common;
use common::*;
use keyboard_identifier::keyboard_provider::DeviceProvider;

#[test]
fn test_no_keyboard() {
    let computer = MockDeviceSource::new();
    let keyboard_list = computer.get_keyboards();
    assert!(keyboard_list.is_empty());
}

#[test]
fn test_plugging_keyboard() {
    let computer = MockDeviceSource::new();

    let keyboard_list = computer.get_keyboards();
    assert!(keyboard_list.get(0).is_none());

    computer.plug_keyboard();
    let keyboard_list = computer.get_keyboards();
    assert!(keyboard_list.get(0).is_some());
}

#[test]
fn test_unplugging_keyboard() {
    let computer = MockDeviceSource::new();
    let keyboard = computer.plug_keyboard();

    let keyboard_list = computer.get_keyboards();
    assert!(keyboard_list.get(0).is_some());

    computer.unplug_keyboard(&keyboard);

    let keyboard_list = computer.get_keyboards();
    assert!(keyboard_list.get(0).is_none());
}

#[test]
fn test_plugging_unplugging_keyboard() {
    let computer = MockDeviceSource::new();

    let keyboard_list = computer.get_keyboards();
    assert!(keyboard_list.get(0).is_none());

    let keyboard = computer.plug_keyboard();
    let keyboard_list = computer.get_keyboards();
    assert!(keyboard_list.get(0).is_some());

    computer.unplug_keyboard(&keyboard);
    let keyboard_list = computer.get_keyboards();
    assert!(keyboard_list.get(0).is_none());
}

#[test]
fn test_one_keyboard() {
    let computer = MockDeviceSource::new();
    computer.plug_keyboard();

    let keyboard_list = computer.get_keyboards();
    assert!(keyboard_list.get(0).is_some());
    assert!(keyboard_list.get(1).is_none());
}

#[test]
fn test_multiple_keyboards() {
    let computer = MockDeviceSource::new();
    for _ in 0..5 {
        computer.plug_keyboard();
    }

    let keyboard_list = computer.get_keyboards();
    (0..5).for_each(|i| assert!(keyboard_list.get(i).is_some()));
    assert!(keyboard_list.get(5).is_none());
}
