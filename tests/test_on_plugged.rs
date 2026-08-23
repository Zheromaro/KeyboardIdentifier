mod common;

use common::*;
use keyboard_identifier::*;
use std::time::Duration;
use tokio::time::timeout;

// --- Helper Functions ---
// (You can move these to common/mod.rs if you want to share them between test files!)

async fn expect_recv<T>(rx: &mut tokio::sync::mpsc::UnboundedReceiver<T>) -> T {
    timeout(Duration::from_millis(100), rx.recv())
        .await
        .expect("Timed out waiting for event")
        .expect("Channel closed unexpectedly")
}

async fn expect_timeout<T>(rx: &mut tokio::sync::mpsc::UnboundedReceiver<T>) {
    let result = timeout(Duration::from_millis(100), rx.recv()).await;
    assert!(
        result.is_err(),
        "Expected timeout, but received an unexpected event"
    );
}

// --- Tests ---

#[tokio::test]
async fn test_no_plugged() {
    let computer = MockDeviceSource::new();
    let listener = KeyboardListener::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    listener.on_plugged(move |_| {
        let _ = tx.send(());
    });
    listener.listen(computer);
    tokio::task::yield_now().await;

    // No keyboards are plugged in, so this should time out.
    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_one_plugged_one_job() {
    let computer = MockDeviceSource::new();
    let listener = KeyboardListener::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    listener.on_plugged(move |_| {
        let _ = tx.send(());
    });
    listener.listen(computer.clone());

    // Crucial: yield to let the listener task start BEFORE plugging the keyboard
    tokio::task::yield_now().await;

    let _keyboard = computer.plug_keyboard();

    expect_recv(&mut rx).await;
    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_two_plugged_one_job() {
    let computer = MockDeviceSource::new();
    let listener = KeyboardListener::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    listener.on_plugged(move |_| {
        let _ = tx.send(());
    });
    listener.listen(computer.clone());
    tokio::task::yield_now().await;

    let _kb1 = computer.plug_keyboard();
    let _kb2 = computer.plug_keyboard();

    expect_recv(&mut rx).await;
    expect_recv(&mut rx).await;
    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_one_plugged_two_jobs() {
    let computer = MockDeviceSource::new();
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

    listener.listen(computer.clone());
    tokio::task::yield_now().await;

    let _keyboard = computer.plug_keyboard();

    let mut results = vec![expect_recv(&mut rx).await, expect_recv(&mut rx).await];
    results.sort();

    assert_eq!(results, vec!["job1", "job2"]);
    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_plugged_id_verification() {
    let computer = MockDeviceSource::new();
    let listener = KeyboardListener::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    listener.on_plugged(move |kb| {
        // We can pass the ID back through the channel to verify it matches
        let _ = tx.send(kb.keyboard_id.clone());
    });

    listener.listen(computer.clone());
    tokio::task::yield_now().await;

    let expected_keyboard = computer.plug_keyboard();

    let received_id = expect_recv(&mut rx).await;
    assert_eq!(received_id, expected_keyboard.keyboard_id);
    expect_timeout(&mut rx).await;
}
