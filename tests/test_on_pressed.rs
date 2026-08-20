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
async fn test_no_press() {
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

#[tokio::test]
async fn test_other_keyboard_press_one_job() {
    let computer = MockDeviceSource::new();
    let keyboard = computer.plug_keyboard();
    let keyboard2 = computer.plug_keyboard();
    let mut listener = KeyboardListener::new();

    let called = Arc::new(AtomicU8::new(0));
    let called_flag = Arc::clone(&called);
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);

    listener.on_pressed(move || {
        called_flag.fetch_add(1, Ordering::SeqCst);
        let _ = tx.blocking_send(());
    });
    listener.listen_to_keyboard(keyboard.clone());
    tokio::task::yield_now().await;
    computer.press(&keyboard2);

    let result = timeout(Duration::from_millis(100), rx.recv()).await;
    assert!(result.is_err());
    assert_eq!(called.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn test_one_press_one_job() {
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
    listener.listen_to_keyboard(keyboard.clone());
    tokio::task::yield_now().await;
    computer.press(&keyboard);

    let result = timeout(Duration::from_millis(100), rx.recv()).await;
    assert!(result.is_ok());
    assert_eq!(called.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_two_presses_one_job() {
    let computer = MockDeviceSource::new();
    let keyboard = computer.plug_keyboard();
    let mut listener = KeyboardListener::new();

    let called = Arc::new(AtomicU8::new(0));
    let called_flag = Arc::clone(&called);
    let (tx, mut rx) = tokio::sync::mpsc::channel(2);

    listener.on_pressed(move || {
        called_flag.fetch_add(1, Ordering::SeqCst);
        let _ = tx.blocking_send(());
    });
    listener.listen_to_keyboard(keyboard.clone());
    tokio::task::yield_now().await;
    computer.press(&keyboard);
    computer.press(&keyboard);

    let result = timeout(Duration::from_millis(100), rx.recv()).await;
    assert!(result.is_ok());
    let result = timeout(Duration::from_millis(100), rx.recv()).await;
    assert!(result.is_ok());
    assert_eq!(called.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn test_one_press_two_jobs() {
    let computer = MockDeviceSource::new();
    let keyboard = computer.plug_keyboard();
    let mut listener = KeyboardListener::new();

    let called = Arc::new(AtomicU8::new(0));
    let called_flag1 = Arc::clone(&called);
    let called_flag2 = Arc::clone(&called);
    let (tx, mut rx) = tokio::sync::mpsc::channel(2);
    let tx2 = tx.clone();

    listener.on_pressed(move || {
        called_flag1.fetch_add(2, Ordering::SeqCst);
        let _ = tx.blocking_send(());
    });
    listener.on_pressed(move || {
        called_flag2.fetch_sub(1, Ordering::SeqCst);
        let _ = tx2.blocking_send(());
    });
    listener.listen_to_keyboard(keyboard.clone());
    tokio::task::yield_now().await;
    computer.press(&keyboard);

    let result = timeout(Duration::from_millis(100), rx.recv()).await;
    assert!(result.is_ok());
    let result = timeout(Duration::from_millis(100), rx.recv()).await;
    assert!(result.is_ok());
    assert_eq!(called.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_two_presses_two_jobs() {
    let computer = MockDeviceSource::new();
    let keyboard = computer.plug_keyboard();
    let mut listener = KeyboardListener::new();

    let called = Arc::new(AtomicU8::new(0));
    let called_flag1 = Arc::clone(&called);
    let called_flag2 = Arc::clone(&called);
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let tx2 = tx.clone();

    listener.on_pressed(move || {
        called_flag1.fetch_add(2, Ordering::SeqCst);
        let _ = tx.blocking_send(());
    });
    listener.on_pressed(move || {
        called_flag2.fetch_sub(1, Ordering::SeqCst);
        let _ = tx2.blocking_send(());
    });
    listener.listen_to_keyboard(keyboard.clone());
    tokio::task::yield_now().await;
    computer.press(&keyboard);
    computer.press(&keyboard);

    for _ in 0..4 {
        let result = timeout(Duration::from_millis(100), rx.recv()).await;
        assert!(result.is_ok());
    }
    assert_eq!(called.load(Ordering::SeqCst), 2);
}
