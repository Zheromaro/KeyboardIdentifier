mod common;

use common::MockKeyboardSource;
use keyboard_identifier::{keyboard_source::KeyboardSource, KeyboardManager};

#[tokio::test]
async fn test_release_keyboard() {
    let source = MockKeyboardSource::new().await.unwrap();
    let manager = KeyboardManager::from(source.clone());
    let keyboard = source.plug_keyboard();

    manager.consume(&keyboard).await.unwrap();
    assert_eq!(manager.get_consumed().len(), 1);

    manager.release(&keyboard).await.unwrap();

    assert!(!source.is_consumed(&keyboard));
    assert!(manager.get_consumed().is_empty());
}

#[tokio::test]
async fn test_release_only_releases_the_requested_keyboard() {
    let source = MockKeyboardSource::new().await.unwrap();
    let manager = KeyboardManager::from(source.clone());
    let keyboard_a = source.plug_keyboard();
    let keyboard_b = source.plug_keyboard();

    manager.consume(&keyboard_a).await.unwrap();
    manager.consume(&keyboard_b).await.unwrap();

    manager.release(&keyboard_a).await.unwrap();

    assert!(!source.is_consumed(&keyboard_a));
    assert!(source.is_consumed(&keyboard_b));

    let consumed = manager.get_consumed();
    assert_eq!(consumed.len(), 1);
    assert_eq!(consumed[0].as_ref(), keyboard_b.as_ref());
}
