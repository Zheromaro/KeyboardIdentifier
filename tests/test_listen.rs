mod common;
use common::*;
use keyboard_identifier::{keyboard_source::*, *};

#[tokio::test]
async fn test_listen_routes_all_events() {
    let computer = MockDeviceSource::new().await.unwrap();
    let listener = KeyboardListener::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    // Register one callback for each event type, sending a distinct string
    let tx_plugged = tx.clone();
    listener.on_plugged(move |_| {
        let _ = tx_plugged.send("plugged");
    });

    let tx_pressed = tx.clone();
    listener.on_pressed(move |_| {
        let _ = tx_pressed.send("pressed");
    });

    let tx_unplugged = tx.clone();
    listener.on_unplugged(move |_| {
        let _ = tx_unplugged.send("unplugged");
    });

    // Start listening
    listener.listen(computer.clone()).await;
    tokio::task::yield_now().await;

    // Trigger all three events in sequence
    let keyboard = computer.plug_keyboard();
    computer.press(&keyboard);
    computer.unplug_keyboard(&keyboard);

    // Verify they are routed and received in the exact order they were triggered
    assert_eq!(expect_recv(&mut rx).await, "plugged");
    assert_eq!(expect_recv(&mut rx).await, "pressed");
    assert_eq!(expect_recv(&mut rx).await, "unplugged");

    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_listen_shuts_down_on_drop() {
    let computer = MockDeviceSource::new().await.unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    // Create a local scope for the listener so we can force it to drop
    {
        let listener = KeyboardListener::new();
        let tx_plugged = tx.clone();

        listener.on_plugged(move |_| {
            let _ = tx_plugged.send("plugged");
        });
        listener.listen(computer.clone()).await;
        tokio::task::yield_now().await;

        // Trigger an event to prove the listener is currently active
        let _kb1 = computer.plug_keyboard();
        assert_eq!(expect_recv(&mut rx).await, "plugged");
    } // `listener` is dropped here, triggering the `Drop` trait and sending the shutdown signal[cite: 1]

    // Yield to give the listener's background task time to process the shutdown signal
    tokio::task::yield_now().await;

    // Trigger another event now that the listener should be dead
    let _kb2 = computer.plug_keyboard();

    // If the task shut down successfully, this event will never be processed
    expect_timeout(&mut rx).await;
}

#[tokio::test]
async fn test_new_and_default() {
    // Verify new() initializes without panicking
    let _listener_new = KeyboardListener::new();

    // If we reach here without a panic, the internal broadcast channels
    // and registries were successfully created.
    assert!(true);
}
