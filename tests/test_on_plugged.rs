mod common;
use common::*;
use keyboard_identifier::keyboard_provider::DeviceProvider;
use keyboard_identifier::*;

#[tokio::test]
async fn test_no_plugged() {
    let computer = MockDeviceSource::new().await;
    let listener = KeyboardListener::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    listener.on_plugged(move |_| {
        let _ = tx.send(());
    });
    listener.listen_with(computer).await;
    tokio::task::yield_now().await;

    // No keyboards are plugged in, so this should time out.
    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_one_plugged_one_job() {
    let computer = MockDeviceSource::new().await;
    let listener = KeyboardListener::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    listener.on_plugged(move |_| {
        let _ = tx.send(());
    });
    listener.listen_with(computer.clone()).await;

    // Crucial: yield to let the listener task start BEFORE plugging the keyboard
    tokio::task::yield_now().await;

    let _keyboard = computer.plug_keyboard();

    expect_recv(&mut rx).await;
    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_two_plugged_one_job() {
    let computer = MockDeviceSource::new().await;
    let listener = KeyboardListener::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    listener.on_plugged(move |_| {
        let _ = tx.send(());
    });
    listener.listen_with(computer.clone()).await;
    tokio::task::yield_now().await;

    let _kb1 = computer.plug_keyboard();
    let _kb2 = computer.plug_keyboard();

    expect_recv(&mut rx).await;
    expect_recv(&mut rx).await;
    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_one_plugged_two_jobs() {
    let computer = MockDeviceSource::new().await;
    let listener = KeyboardListener::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let tx1 = tx.clone();
    listener.on_plugged(move |_| {
        let _ = tx1.send("job1");
    });

    let tx2 = tx.clone();
    listener.on_plugged(move |_| {
        let _ = tx2.send("job2");
    });

    listener.listen_with(computer.clone()).await;
    tokio::task::yield_now().await;

    let _keyboard = computer.plug_keyboard();

    let mut results = vec![expect_recv(&mut rx).await, expect_recv(&mut rx).await];
    results.sort();

    assert_eq!(results, vec!["job1", "job2"]);
    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_plugged_id_verification() {
    let computer = MockDeviceSource::new().await;
    let listener = KeyboardListener::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    listener.on_plugged(move |kb| {
        // We can pass the ID back through the channel to verify it matches
        let _ = tx.send(kb.keyboard_id.clone());
    });

    listener.listen_with(computer.clone()).await;
    tokio::task::yield_now().await;

    let expected_keyboard = computer.plug_keyboard();

    let received_id = expect_recv(&mut rx).await;
    assert_eq!(received_id, expected_keyboard.keyboard_id);
    expect_timeout(&mut rx).await;
}
