mod common;

use common::MockKeyboardSource;
use keyboard_identifier::{keyboard_source::KeyboardSource, KeyboardManager};

#[tokio::test]
async fn test_get_consumed_returns_empty_when_no_keyboard_is_consumed() {
    let source = MockKeyboardSource::new().await.unwrap();
    let manager = KeyboardManager::from(source);

    assert!(manager.get_consumed().is_empty());
}

#[tokio::test]
async fn test_get_consumed_returns_all_consumed_keyboards() {
    let source = MockKeyboardSource::new().await.unwrap();
    let manager = KeyboardManager::from(source.clone());
    let keyboard_a = source.plug_keyboard();
    let keyboard_b = source.plug_keyboard();

    manager.consume(&keyboard_a).await.unwrap();
    manager.consume(&keyboard_b).await.unwrap();

    let consumed = manager.get_consumed();

    assert_eq!(consumed.len(), 2);
    assert!(consumed.iter().any(|keyboard| keyboard.as_ref() == keyboard_a.as_ref()));
    assert!(consumed.iter().any(|keyboard| keyboard.as_ref() == keyboard_b.as_ref()));
}
