mod common;
use common::*;
use keyboard_identifier::KeyboardManager;

#[tokio::test]
async fn test_no_keyboards() {
    let computer = MockDeviceSource::new().await.unwrap();

    assert!(computer.get_keyboards().is_empty());
}

#[tokio::test]
async fn test_plug_single_keyboard() {
    let computer = MockDeviceSource::new().await.unwrap();

    let plugged_keyboard = computer.plug_keyboard();
    let keyboard_list = computer.get_keyboards();

    assert_eq!(keyboard_list.len(), 1);
    assert_eq!(keyboard_list[0], plugged_keyboard);
}
#[tokio::test]
async fn test_unplug_keyboard() {
    let computer = MockDeviceSource::new().await.unwrap();
    let plugged_keyboard = computer.plug_keyboard();

    assert_eq!(computer.get_keyboards().len(), 1);

    computer.unplug_keyboard(&plugged_keyboard);

    assert!(computer.get_keyboards().is_empty());
}

#[tokio::test]
async fn test_multiple_keyboards() {
    let computer = MockDeviceSource::new().await.unwrap();
    let expected_keyboards: Vec<_> = (0..5).map(|_| computer.plug_keyboard()).collect();

    let keyboard_list = computer.get_keyboards();

    assert_eq!(keyboard_list.len(), 5);
    assert_eq!(keyboard_list, expected_keyboards);
}

#[tokio::test]
async fn test_unplug_specific_keyboard_among_many() {
    let computer = MockDeviceSource::new().await.unwrap();
    let kb1 = computer.plug_keyboard();
    let kb2 = computer.plug_keyboard();
    let kb3 = computer.plug_keyboard();

    computer.unplug_keyboard(&kb2);

    let keyboard_list = computer.get_keyboards();

    assert_eq!(keyboard_list.len(), 2);
    assert_eq!(keyboard_list, vec![kb1, kb3]);
}
