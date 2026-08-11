mod common;
use common::*;
use keyboard_identifier::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
        mpsc,
    },
    time::Duration,
};

#[test]
fn test_keyboard_not_pressed() {
    let computer = MockDeviceSource::new();
    let keyboard = computer.plug_keyboard();
    let mut listener = KeyboardListener::new(keyboard);

    let (tx, rx) = mpsc::channel();
    let called = Arc::new(AtomicU8::new(0));
    let called_flag = called.clone();
    listener.on_pressed(move || {
        called_flag.fetch_add(1, Ordering::SeqCst);
        let _ = tx.send(());
    });
    listener.start_listener();

    assert!(rx.recv_timeout(Duration::from_millis(50)).is_err());
    assert_eq!(called.load(Ordering::SeqCst), 0);
}

#[test]
fn test_keyboard_one_press() {
    let computer = MockDeviceSource::new();
    let keyboard = computer.plug_keyboard();
    let mut listener = KeyboardListener::new(keyboard.clone());

    let (tx, rx) = mpsc::channel();
    let called = Arc::new(AtomicU8::new(0));
    let called_flag = called.clone();

    listener.on_pressed(move || {
        called_flag.fetch_add(1, Ordering::SeqCst);
        let _ = tx.send(());
    });
    listener.start_listener();

    computer.press(&keyboard);

    assert!(rx.recv_timeout(Duration::from_millis(50)).is_ok());
    assert_eq!(called.load(Ordering::SeqCst), 1);
}

#[test]
fn test_keyboard_two_press() {
    let computer = MockDeviceSource::new();
    let keyboard = computer.plug_keyboard();
    let mut listener = KeyboardListener::new(keyboard.clone());

    let (tx, rx) = mpsc::channel();
    let called = Arc::new(AtomicU8::new(0));
    let called_flag = called.clone();

    listener.on_pressed(move || {
        called_flag.fetch_add(1, Ordering::SeqCst);
        let _ = tx.send(());
    });
    listener.start_listener();

    computer.press(&keyboard);
    computer.press(&keyboard);

    assert!(rx.recv_timeout(Duration::from_millis(50)).is_ok());
    assert!(rx.recv_timeout(Duration::from_millis(50)).is_ok());
    assert_eq!(called.load(Ordering::SeqCst), 2);
}

#[test]
fn test_two_keyboard_pressed() {
    let computer = MockDeviceSource::new();
    let keyboard1 = computer.plug_keyboard();
    let keyboard2 = computer.plug_keyboard();
    let mut listener = KeyboardListener::new(keyboard1.clone());

    let (tx, rx) = mpsc::channel();
    let called = Arc::new(AtomicU8::new(0));
    let called_flag = called.clone();

    listener.on_pressed(move || {
        called_flag.fetch_add(1, Ordering::SeqCst);
        let _ = tx.send(());
    });
    listener.start_listener();

    computer.press(&keyboard1);
    computer.press(&keyboard2);

    assert!(rx.recv_timeout(Duration::from_millis(50)).is_ok());
    assert_eq!(called.load(Ordering::SeqCst), 1);
}

#[test]
fn test_other_keyboard_pressed() {
    let computer = MockDeviceSource::new();
    let keyboard1 = computer.plug_keyboard();
    let keyboard2 = computer.plug_keyboard();
    let mut listener = KeyboardListener::new(keyboard1.clone());

    let (tx, rx) = mpsc::channel();
    let called = Arc::new(AtomicU8::new(0));
    let called_flag = called.clone();

    listener.on_pressed(move || {
        called_flag.fetch_add(1, Ordering::SeqCst);
        let _ = tx.send(());
    });

    computer.press(&keyboard2);

    assert!(rx.recv_timeout(Duration::from_millis(50)).is_err());
    assert_eq!(called.load(Ordering::SeqCst), 0);
}

#[test]
fn test_keyboard_one_press_two_jobs() {
    let computer = MockDeviceSource::new();
    let keyboard = computer.plug_keyboard();
    let mut listener = KeyboardListener::new(keyboard.clone());

    let (tx, rx) = mpsc::channel();
    let tx2 = tx.clone();
    let called = Arc::new(AtomicU8::new(0));
    let called_flag1 = called.clone();
    let called_flag2 = called.clone();

    listener.on_pressed(move || {
        called_flag1.fetch_add(2, Ordering::SeqCst);
        let _ = tx.send(());
    });
    listener.on_pressed(move || {
        called_flag2.fetch_sub(1, Ordering::SeqCst);
        let _ = tx2.send(());
    });
    listener.start_listener();

    computer.press(&keyboard);
    assert!(rx.recv_timeout(Duration::from_millis(50)).is_ok());
    assert_eq!(called.load(Ordering::SeqCst), 1);
}

#[test]
fn test_keyboard_two_press_two_jobs() {
    let computer = MockDeviceSource::new();
    let keyboard = computer.plug_keyboard();
    let mut listener = KeyboardListener::new(keyboard.clone());

    let (tx, rx) = mpsc::channel();
    let tx2 = tx.clone();
    let called = Arc::new(AtomicU8::new(0));
    let called_flag1 = called.clone();
    let called_flag2 = called.clone();

    listener.on_pressed(move || {
        called_flag1.fetch_add(2, Ordering::SeqCst);
        let _ = tx.send(());
    });
    listener.on_pressed(move || {
        called_flag2.fetch_sub(1, Ordering::SeqCst);
        let _ = tx2.send(());
    });
    listener.start_listener();

    computer.press(&keyboard);
    computer.press(&keyboard);

    assert!(rx.recv_timeout(Duration::from_millis(50)).is_ok());
    assert!(rx.recv_timeout(Duration::from_millis(50)).is_ok());

    assert_eq!(called.load(Ordering::SeqCst), 2);
}
