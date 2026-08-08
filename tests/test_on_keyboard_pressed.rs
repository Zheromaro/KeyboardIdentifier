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

    let (tx, rx) = mpsc::channel();
    let called = Arc::new(AtomicU8::new(0));
    let called_flag = called.clone();

    on_keyboard_pressed(
        move || {
            called_flag.fetch_add(1, Ordering::SeqCst);
            let _ = tx.send(());
        },
        &computer,
        &keyboard,
    );

    assert!(rx.recv_timeout(Duration::from_millis(50)).is_err());
    assert_eq!(called.load(Ordering::SeqCst), 0);
}

#[test]
fn test_keyboard_one_press() {
    let computer = MockDeviceSource::new();
    let keyboard = computer.plug_keyboard();

    let (tx, rx) = mpsc::channel();
    let called = Arc::new(AtomicU8::new(0));
    let called_flag = called.clone();

    on_keyboard_pressed(
        move || {
            called_flag.fetch_add(1, Ordering::SeqCst);
            let _ = tx.send(());
        },
        &computer,
        &keyboard,
    );

    computer.press(&keyboard);

    assert!(rx.recv_timeout(Duration::from_millis(50)).is_ok());
    assert_eq!(called.load(Ordering::SeqCst), 1);
}

#[test]
fn test_keyboard_two_press() {
    let computer = MockDeviceSource::new();
    let keyboard = computer.plug_keyboard();

    let (tx, rx) = mpsc::channel();
    let called = Arc::new(AtomicU8::new(0));
    let called_flag = called.clone();

    on_keyboard_pressed(
        move || {
            called_flag.fetch_add(1, Ordering::SeqCst);
            let _ = tx.send(());
        },
        &computer,
        &keyboard,
    );

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

    let (tx, rx) = mpsc::channel();
    let called = Arc::new(AtomicU8::new(0));
    let called_flag = called.clone();

    on_keyboard_pressed(
        move || {
            called_flag.fetch_add(1, Ordering::SeqCst);
            let _ = tx.send(());
        },
        &computer,
        &keyboard1,
    );

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

    let (tx, rx) = mpsc::channel();
    let called = Arc::new(AtomicU8::new(0));
    let called_flag = called.clone();

    on_keyboard_pressed(
        move || {
            called_flag.fetch_add(1, Ordering::SeqCst);
            let _ = tx.send(());
        },
        &computer,
        &keyboard1,
    );

    computer.press(&keyboard2);

    assert!(rx.recv_timeout(Duration::from_millis(50)).is_err());
    assert_eq!(called.load(Ordering::SeqCst), 0);
}
