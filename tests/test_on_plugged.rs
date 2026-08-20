mod common;

use common::*;
use keyboard_identifier::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::Duration,
};
use tokio::time::timeout;

#[tokio::test]
async fn test_not_plugged() {
    let computer = MockDeviceSource::new();
    let keyboard = computer.plug_keyboard();
    let mut listener = KeyboardListener::new();

    let called = Arc::new(AtomicU8::new(0));
    let called_flag = Arc::clone(&called);
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);

    listener.on_pressed(move || {
        called_flag.fetch_add(1, Ordering::SeqCst);
        let _ = tx.blocking_send(());
    });
    listener.listen_to_keyboard(keyboard);
    tokio::task::yield_now().await;

    let result = timeout(Duration::from_millis(100), rx.recv()).await;
    assert!(result.is_err());
    assert_eq!(called.load(Ordering::SeqCst), 0);
}
