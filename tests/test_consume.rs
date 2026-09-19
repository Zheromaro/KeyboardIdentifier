mod common;

use common::MockKeyboardSource;
use keyboard_identifier::{keyboard_source::KeyboardSource, KeyboardManager};

#[tokio::test]
async fn test_consume_keyboard() {
    let source = MockKeyboardSource::new().await.unwrap();
    let manager = KeyboardManager::from(source.clone());
    let keyboard = source.plug_keyboard();

    manager.consume(&keyboard).await.unwrap();

    assert!(source.is_consumed(&keyboard));

    let consumed = manager.get_consumed();
    assert_eq!(consumed.len(), 1);
    assert_eq!(consumed[0].as_ref(), keyboard.as_ref());
}

#[tokio::test]
async fn test_consume_same_keyboard_is_idempotent() {
    let source = MockKeyboardSource::new().await.unwrap();
    let manager = KeyboardManager::from(source.clone());
    let keyboard = source.plug_keyboard();

    manager.consume(&keyboard).await.unwrap();
    manager.consume(&keyboard).await.unwrap();

    assert_eq!(manager.get_consumed().len(), 1);
    assert!(source.is_consumed(&keyboard));
}
