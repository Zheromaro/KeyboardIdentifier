mod common;
use common::*;
use keyboard_identifier::keyboard_provider::DeviceProvider;
use keyboard_identifier::*;

#[tokio::test]
async fn test_no_unplug() {
    let computer = MockDeviceSource::new().await;
    let _keyboard = computer.plug_keyboard();
    let listener = KeyboardListener::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    listener.on_unplugged(move |_| {
        let _ = tx.send(());
    });
    listener.listen_with(computer).await;
    tokio::task::yield_now().await;

    // Keyboard was plugged in, but never unplugged
    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_one_unplug_one_job() {
    let computer = MockDeviceSource::new().await;
    let keyboard = computer.plug_keyboard();
    let listener = KeyboardListener::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    listener.on_unplugged(move |_| {
        let _ = tx.send(());
    });
    listener.listen_with(computer.clone()).await;
    tokio::task::yield_now().await;

    computer.unplug_keyboard(&keyboard);

    expect_recv(&mut rx).await;
    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_two_unplugs_one_job() {
    let computer = MockDeviceSource::new().await;
    let kb1 = computer.plug_keyboard();
    let kb2 = computer.plug_keyboard();
    let listener = KeyboardListener::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    listener.on_unplugged(move |_| {
        let _ = tx.send(());
    });
    listener.listen_with(computer.clone()).await;
    tokio::task::yield_now().await;

    computer.unplug_keyboard(&kb1);
    computer.unplug_keyboard(&kb2);

    expect_recv(&mut rx).await;
    expect_recv(&mut rx).await;
    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_one_unplug_two_jobs() {
    let computer = MockDeviceSource::new().await;
    let keyboard = computer.plug_keyboard();
    let listener = KeyboardListener::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let tx1 = tx.clone();
    listener.on_unplugged(move |_| {
        let _ = tx1.send("job1");
    });

    let tx2 = tx.clone();
    listener.on_unplugged(move |_| {
        let _ = tx2.send("job2");
    });

    listener.listen_with(computer.clone()).await;
    tokio::task::yield_now().await;

    computer.unplug_keyboard(&keyboard);

    let mut results = vec![expect_recv(&mut rx).await, expect_recv(&mut rx).await];
    results.sort();

    assert_eq!(results, vec!["job1", "job2"]);
    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_unplugged_id_verification() {
    let computer = MockDeviceSource::new().await;
    let keyboard = computer.plug_keyboard();
    let listener = KeyboardListener::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    listener.on_unplugged(move |kb| {
        let _ = tx.send(kb.keyboard_id.clone());
    });

    listener.listen_with(computer.clone()).await;
    tokio::task::yield_now().await;

    computer.unplug_keyboard(&keyboard);

    let received_id = expect_recv(&mut rx).await;
    assert_eq!(received_id, keyboard.keyboard_id);
    expect_timeout(&mut rx).await;
}
