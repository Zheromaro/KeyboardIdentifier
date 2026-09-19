mod common;

use common::MockKeyboardSource;
use keyboard_identifier::{keyboard_source::KeyboardSource, KeyboardManager};

#[tokio::test]
async fn test_release_all_releases_every_consumed_keyboard() {
    let source = MockKeyboardSource::new().await.unwrap();
    let manager = KeyboardManager::from(source.clone());
    let keyboard_a = source.plug_keyboard();
    let keyboard_b = source.plug_keyboard();
    let keyboard_c = source.plug_keyboard();

    manager.consume(&keyboard_a).await.unwrap();
    manager.consume(&keyboard_b).await.unwrap();
    manager.consume(&keyboard_c).await.unwrap();

    assert_eq!(manager.get_consumed().len(), 3);

    manager.release_all().await.unwrap();

    assert!(manager.get_consumed().is_empty());
    assert!(!source.is_consumed(&keyboard_a));
    assert!(!source.is_consumed(&keyboard_b));
    assert!(!source.is_consumed(&keyboard_c));
}

#[tokio::test]
async fn test_release_all_succeeds_when_nothing_is_consumed() {
    let source = MockKeyboardSource::new().await.unwrap();
    let manager = KeyboardManager::from(source);

    manager.release_all().await.unwrap();

    assert!(manager.get_consumed().is_empty());
}
