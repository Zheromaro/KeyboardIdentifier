mod common;

use common::*;
use keyboard_identifier::*;

#[tokio::test]
async fn test_no_press() {
    let computer = MockDeviceSource::new();
    let listener = KeyboardListener::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    listener.on_pressed(move |_| {
        let _ = tx.send(());
    });
    listener.listen(computer);
    tokio::task::yield_now().await;

    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_other_keyboard_press_one_job() {
    let computer = MockDeviceSource::new();
    let keyboard = computer.plug_keyboard();
    let keyboard2 = computer.plug_keyboard();
    let listener = KeyboardListener::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    // FIX: Clone the ID before moving it into the closure
    let target_id = keyboard.keyboard_id.clone();

    listener.on_pressed(move |kb| {
        if kb.keyboard_id == target_id {
            let _ = tx.send(());
        }
    });

    listener.listen(computer.clone());
    tokio::task::yield_now().await;
    computer.press(&keyboard2);

    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_one_press_one_job() {
    let computer = MockDeviceSource::new();
    let keyboard = computer.plug_keyboard();
    let listener = KeyboardListener::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    listener.on_pressed(move |_| {
        let _ = tx.send(());
    });
    listener.listen(computer.clone());
    tokio::task::yield_now().await;
    computer.press(&keyboard);

    expect_recv(&mut rx).await;
    expect_timeout(&mut rx).await; // Verify no extra presses occurred
}

#[tokio::test]
async fn test_two_presses_one_job() {
    let computer = MockDeviceSource::new();
    let keyboard = computer.plug_keyboard();
    let listener = KeyboardListener::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    listener.on_pressed(move |_| {
        let _ = tx.send(());
    });
    listener.listen(computer.clone());
    tokio::task::yield_now().await;

    computer.press(&keyboard);
    computer.press(&keyboard);

    expect_recv(&mut rx).await;
    expect_recv(&mut rx).await;
    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_one_press_two_jobs() {
    let computer = MockDeviceSource::new();
    let keyboard = computer.plug_keyboard();
    let listener = KeyboardListener::new();

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let target_id = keyboard.keyboard_id.clone();
    let tx1 = tx.clone();
    listener.on_pressed(move |kb| {
        if kb.keyboard_id == target_id {
            let _ = tx1.send("job1");
        }
    });

    let tx2 = tx.clone();
    listener.on_pressed(move |_| {
        let _ = tx2.send("job2");
    });

    listener.listen(computer.clone());
    tokio::task::yield_now().await;
    computer.press(&keyboard);

    let mut results = vec![expect_recv(&mut rx).await, expect_recv(&mut rx).await];
    results.sort(); // Sort because concurrent job execution order isn't guaranteed

    assert_eq!(results, vec!["job1", "job2"]);
    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_two_presses_two_jobs() {
    let computer = MockDeviceSource::new();
    let keyboard = computer.plug_keyboard();
    let listener = KeyboardListener::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let tx1 = tx.clone();
    listener.on_pressed(move |_| {
        let _ = tx1.send("job1");
    });

    let tx2 = tx.clone();
    listener.on_pressed(move |_| {
        let _ = tx2.send("job2");
    });

    listener.listen(computer.clone());
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
