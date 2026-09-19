mod common;

use common::expect_recv;
use common::MockKeyboardSource;
use keyboard_identifier::{
    keyboard_source::{KeyboardEvent, KeyboardSource},
    KeyboardManager,
};
use keyboard_types::{Code, Key, KeyState};

#[tokio::test]
async fn test_subscribe_receives_plugged_event() {
    let source = MockKeyboardSource::new().await.unwrap();
    let manager = KeyboardManager::from(source.clone());
    let mut events = manager.subscribe();

    manager.listen().await;
    tokio::task::yield_now().await;

    let keyboard = source.plug_keyboard();
    let event = expect_recv(&mut events).await;

    match event {
        KeyboardEvent::Plugged(received) => {
            assert_eq!(received.as_ref(), keyboard.as_ref());
        }
        other => panic!("Expected Plugged event, got {other:?}"),
    }
}

#[tokio::test]
async fn test_subscribe_receives_key_action_event() {
    let source = MockKeyboardSource::new().await.unwrap();
    let manager = KeyboardManager::from(source.clone());
    let mut events = manager.subscribe();

    manager.listen().await;
    tokio::task::yield_now().await;

    let keyboard = source.plug_keyboard();
    let _ = expect_recv(&mut events).await;

    source.press_key(&keyboard, Key::Character("a".into()), Code::KeyA);

    let event = expect_recv(&mut events).await;

    match event {
        KeyboardEvent::KeyAction(received, key_event) => {
            assert_eq!(received.as_ref(), keyboard.as_ref());
            assert_eq!(key_event.state, KeyState::Down);
            assert_eq!(key_event.key, Key::Character("a".into()));
            assert_eq!(key_event.code, Code::KeyA);
        }
        other => panic!("Expected KeyAction event, got {other:?}"),
    }
}

#[tokio::test]
async fn test_subscribe_receives_unplugged_event() {
    let source = MockKeyboardSource::new().await.unwrap();
    let manager = KeyboardManager::from(source.clone());
    let mut events = manager.subscribe();

    manager.listen().await;
    tokio::task::yield_now().await;

    let keyboard = source.plug_keyboard();
    let _ = expect_recv(&mut events).await;

    source.unplug_keyboard(&keyboard);

    let event = expect_recv(&mut events).await;

    match event {
        KeyboardEvent::Unplugged(received) => {
            assert_eq!(received.as_ref(), keyboard.as_ref());
        }
        other => panic!("Expected Unplugged event, got {other:?}"),
    }
}
