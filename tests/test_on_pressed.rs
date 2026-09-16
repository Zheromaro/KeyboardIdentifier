mod common;
use common::*;
use keyboard_identifier::{keyboard_source::*, *};
use keyboard_types::{Code, Key, KeyState};

#[tokio::test]
async fn test_no_press() {
    let computer = MockDeviceSource::new().await.unwrap();
    let mut listener = KeyboardManager::from(computer.clone());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    listener.on_key_action(move |_, _| {
        let _ = tx.send(());
    });
    listener.listen().await;
    tokio::task::yield_now().await;

    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_other_keyboard_press_one_job() {
    let computer = MockDeviceSource::new().await.unwrap();
    let keyboard = computer.plug_keyboard();
    let keyboard2 = computer.plug_keyboard();
    let mut listener = KeyboardManager::from(computer.clone());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    listener.on_key_action(move |kb, _| {
        if kb.keyboard_id == keyboard.keyboard_id {
            let _ = tx.send(());
        }
    });

    listener.listen().await;
    tokio::task::yield_now().await;
    computer.press(&keyboard2);

    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_one_press_one_job() {
    let computer = MockDeviceSource::new().await.unwrap();
    let keyboard = computer.plug_keyboard();
    let mut listener = KeyboardManager::from(computer.clone());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    listener.on_key_action(move |_, _| {
        let _ = tx.send(());
    });
    listener.listen().await;
    computer.press(&keyboard);

    expect_recv(&mut rx).await;
    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_two_presses_one_job() {
    let computer = MockDeviceSource::new().await.unwrap();
    let keyboard = computer.plug_keyboard();
    let mut listener = KeyboardManager::from(computer.clone());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    listener.on_key_action(move |_, _| {
        let _ = tx.send(());
    });
    listener.listen().await;
    tokio::task::yield_now().await;

    computer.press(&keyboard);
    computer.press(&keyboard);

    expect_recv(&mut rx).await;
    expect_recv(&mut rx).await;
    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_one_press_two_jobs() {
    let computer = MockDeviceSource::new().await.unwrap();
    let keyboard = computer.plug_keyboard();
    let mut listener = KeyboardManager::from(computer.clone());

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let target_id = keyboard.keyboard_id.clone();
    let tx1 = tx.clone();
    listener.on_key_action(move |kb, _| {
        if kb.keyboard_id == target_id {
            let _ = tx1.send("job1");
        }
    });

    let tx2 = tx.clone();
    listener.on_key_action(move |_, _| {
        let _ = tx2.send("job2");
    });

    listener.listen().await;
    tokio::task::yield_now().await;
    computer.press(&keyboard);

    let mut results = vec![expect_recv(&mut rx).await, expect_recv(&mut rx).await];
    results.sort(); // Sort because concurrent job execution order isn't guaranteed

    assert_eq!(results, vec!["job1", "job2"]);
    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_two_presses_two_jobs() {
    let computer = MockDeviceSource::new().await.unwrap();
    let keyboard = computer.plug_keyboard();
    let mut listener = KeyboardManager::from(computer.clone());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let tx1 = tx.clone();
    listener.on_key_action(move |_, _| {
        let _ = tx1.send("job1");
    });

    let tx2 = tx.clone();
    listener.on_key_action(move |_, _| {
        let _ = tx2.send("job2");
    });

    listener.listen().await;
    tokio::task::yield_now().await;

    computer.press(&keyboard);
    computer.press(&keyboard);

    let mut results = Vec::new();
    for _ in 0..4 {
        results.push(expect_recv(&mut rx).await);
    }

    assert_eq!(results.iter().filter(|&&s| s == "job1").count(), 2);
    assert_eq!(results.iter().filter(|&&s| s == "job2").count(), 2);
    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_release_triggers_callback_with_up_state() {
    let computer = MockDeviceSource::new().await.unwrap();
    let keyboard = computer.plug_keyboard();
    let mut listener = KeyboardManager::from(computer.clone());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    listener.on_key_action(move |_, event| {
        let _ = tx.send(event.state);
    });
    listener.listen().await;
    tokio::task::yield_now().await;

    computer.release(&keyboard);

    assert_eq!(expect_recv(&mut rx).await, KeyState::Up);
    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_key_event_data() {
    let computer = MockDeviceSource::new().await.unwrap();
    let keyboard = computer.plug_keyboard();
    let mut listener = KeyboardManager::from(computer.clone());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    listener.on_key_action(move |_, event| {
        let _ = tx.send((event.key.clone(), event.code, event.state));
    });
    listener.listen().await;
    tokio::task::yield_now().await;

    computer.press_key(&keyboard, Key::Character("b".into()), Code::KeyB);

    assert_eq!(
        expect_recv(&mut rx).await,
        (Key::Character("b".into()), Code::KeyB, KeyState::Down)
    );
    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_press_after_unplug_no_event() {
    let computer = MockDeviceSource::new().await.unwrap();
    let keyboard = computer.plug_keyboard();
    let mut listener = KeyboardManager::from(computer.clone());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    listener.on_key_action(move |_, _| {
        let _ = tx.send(());
    });
    listener.listen().await;
    tokio::task::yield_now().await;

    computer.unplug_keyboard(&keyboard);
    computer.press(&keyboard);

    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_plug_and_unplug_callbacks() {
    let computer = MockDeviceSource::new().await.unwrap();
    let mut listener = KeyboardManager::from(computer.clone());
    let (plug_tx, mut plug_rx) = tokio::sync::mpsc::unbounded_channel();
    let (unplug_tx, mut unplug_rx) = tokio::sync::mpsc::unbounded_channel();

    listener.on_plugged(move |kb| {
        let _ = plug_tx.send(kb.keyboard_id.name.clone());
    });
    listener.on_unplugged(move |kb| {
        let _ = unplug_tx.send(kb.keyboard_id.name.clone());
    });
    listener.listen().await;
    tokio::task::yield_now().await;

    let keyboard = computer.plug_keyboard();
    let name = keyboard.keyboard_id.name.clone();

    assert_eq!(expect_recv(&mut plug_rx).await, name);
    expect_timeout(&mut unplug_rx).await;

    computer.unplug_keyboard(&keyboard);

    assert_eq!(expect_recv(&mut unplug_rx).await, name);
    expect_timeout(&mut plug_rx).await;
}

#[tokio::test]
async fn test_get_keyboards_tracks_plug_unplug() {
    let computer = MockDeviceSource::new().await.unwrap();
    let keyboard = computer.plug_keyboard();
    let mut listener = KeyboardManager::from(computer.clone());
    listener.listen().await;

    assert!(listener.get_keyboards().contains(&keyboard));

    computer.unplug_keyboard(&keyboard);
    tokio::task::yield_now().await;

    assert!(!listener.get_keyboards().contains(&keyboard));
}
