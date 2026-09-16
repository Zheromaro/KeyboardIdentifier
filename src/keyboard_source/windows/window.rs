use super::{InterceptionCommand, device::DeviceEnumerator, key_mapping::*};
use crate::keyboard_source::{Keyboard, KeyboardEvent};
use interception::{Filter, Interception, KeyFilter, KeyState, ScanCode};
use std::{
    collections::{HashMap, HashSet},
    io,
    sync::{Arc, RwLock, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tokio::sync::oneshot;
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;

const POLL_INTERVAL: Duration = Duration::from_millis(25);
const DEVICE_REFRESH_INTERVAL: Duration = Duration::from_millis(500);

pub(crate) struct InputThread {
    command_tx: mpsc::Sender<InterceptionCommand>,
    thread: Option<JoinHandle<()>>,
}

impl InputThread {
    pub(crate) fn spawn(
        event_tx: tokio::sync::mpsc::Sender<Result<KeyboardEvent, io::Error>>,
        keyboards: Arc<RwLock<Vec<Keyboard>>>,
        init_tx: oneshot::Sender<io::Result<()>>,
    ) -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let thread = thread::spawn(move || {
            let result = run_interception(command_rx, event_tx, keyboards);
            let _ = init_tx.send(result);
        });
        Self {
            command_tx,
            thread: Some(thread),
        }
    }

    pub(crate) fn send_command(&self, command: InterceptionCommand) -> io::Result<()> {
        self.command_tx
            .send(command)
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "Windows input thread exited"))
    }
}

impl Drop for InputThread {
    fn drop(&mut self) {
        let _ = self.command_tx.send(InterceptionCommand::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct InterceptionState {
    context: Interception,
    keyboards: HashMap<interception::Device, Arc<Keyboard>>,
    consumed: HashSet<interception::Device>, // FIXED: Syntax error corrected
    pressed_keys: HashSet<(interception::Device, ScanCode)>,
    modifiers: keyboard_types::Modifiers,
    last_device_refresh: Instant,
}

fn run_interception(
    command_rx: mpsc::Receiver<InterceptionCommand>,
    event_tx: tokio::sync::mpsc::Sender<Result<KeyboardEvent, io::Error>>,
    keyboard_snapshot: Arc<RwLock<Vec<Keyboard>>>,
) -> io::Result<()> {
    let Some(context) = Interception::new() else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "failed to create Interception context; make sure the Interception driver is installed",
        ));
    };

    context.set_filter(
        interception::is_keyboard,
        Filter::KeyFilter(KeyFilter::DOWN | KeyFilter::UP | KeyFilter::E0 | KeyFilter::E1),
    );

    // FIXED: Initialize modifier state from OS to handle pre-held/toggled keys
    let mut mods = keyboard_types::Modifiers::empty();
    let is_key_toggled = |vk: i32| unsafe { (GetKeyState(vk) & 1) != 0 };
    if is_key_toggled(0x14) {
        mods.insert(keyboard_types::Modifiers::CAPS_LOCK);
    } // VK_CAPITAL
    if is_key_toggled(0x90) {
        mods.insert(keyboard_types::Modifiers::NUM_LOCK);
    } // VK_NUMLOCK
    if is_key_toggled(0x91) {
        mods.insert(keyboard_types::Modifiers::SCROLL_LOCK);
    } // VK_SCROLL

    let mut state = InterceptionState {
        context,
        keyboards: HashMap::new(),
        consumed: HashSet::new(),
        pressed_keys: HashSet::new(),
        modifiers: mods,
        last_device_refresh: Instant::now() - DEVICE_REFRESH_INTERVAL,
    };

    refresh_devices(&mut state, &event_tx, &keyboard_snapshot, false)?;

    loop {
        if process_commands(&mut state, &command_rx)? {
            break;
        }

        // FIXED: Graceful shutdown if main thread drops the receiver
        if event_tx.is_closed() {
            break;
        }

        let device = state.context.wait_with_timeout(POLL_INTERVAL);
        if interception::is_invalid(device) {
            if state.last_device_refresh.elapsed() >= DEVICE_REFRESH_INTERVAL {
                refresh_devices(&mut state, &event_tx, &keyboard_snapshot, true)?;
            }
            continue;
        }

        if !interception::is_keyboard(device) {
            continue;
        }

        let mut strokes = [interception::Stroke::Keyboard {
            code: ScanCode::Esc,
            state: KeyState::UP,
            information: 0,
        }];
        let received = state.context.receive(device, &mut strokes);
        if received <= 0 {
            continue;
        }

        let Some(keyboard) = state.keyboards.get(&device).cloned() else {
            refresh_devices(&mut state, &event_tx, &keyboard_snapshot, true)?;
            continue;
        };

        for stroke in strokes.into_iter().take(received as usize) {
            let interception::Stroke::Keyboard {
                code,
                state: key_state,
                ..
            } = stroke
            else {
                continue;
            };

            let is_up = key_state.contains(KeyState::UP);
            let key_tuple = (device, code);
            let repeat = if is_up {
                state.pressed_keys.remove(&key_tuple);
                false
            } else {
                !state.pressed_keys.insert(key_tuple)
            };

            update_modifiers(&mut state.modifiers, code, key_state, repeat);
            let event = interception_to_key_event(code, key_state, state.modifiers, repeat);

            // FIXED: Break cleanly if receiver is dropped instead of silently ignoring
            if event_tx
                .blocking_send(Ok(KeyboardEvent::KeyAction(Arc::clone(&keyboard), event)))
                .is_err()
            {
                break;
            }

            if !state.consumed.contains(&device) {
                let sent = state.context.send(device, &[stroke]);
                if sent != 1 {
                    return Err(io::Error::other(format!(
                        "Interception failed to forward keyboard stroke (returned {sent})"
                    )));
                }
            }
        }

        if event_tx.is_closed() {
            break;
        }

        if state.last_device_refresh.elapsed() >= DEVICE_REFRESH_INTERVAL {
            refresh_devices(&mut state, &event_tx, &keyboard_snapshot, true)?;
        }
    }
    Ok(())
}

/// Returns `true` when the worker should stop.
fn process_commands(
    state: &mut InterceptionState,
    command_rx: &mpsc::Receiver<InterceptionCommand>,
) -> io::Result<bool> {
    while let Ok(command) = command_rx.try_recv() {
        match command {
            InterceptionCommand::Consume { keyboard, reply } => {
                let result = set_consumed(state, &keyboard, true);
                let _ = reply.send(result);
            }
            InterceptionCommand::Release { keyboard, reply } => {
                let result = set_consumed(state, &keyboard, false);
                let _ = reply.send(result);
            }
            InterceptionCommand::Stop => return Ok(true),
        }
    }
    Ok(false)
}

fn set_consumed(
    state: &mut InterceptionState,
    keyboard: &Keyboard,
    consumed: bool,
) -> io::Result<()> {
    let device = state
        .keyboards
        .iter()
        .find(|(_, current)| current.as_ref() == keyboard)
        .map(|(device, _)| *device)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "keyboard is not currently connected",
            )
        })?;

    if consumed {
        state.consumed.insert(device);
    } else {
        state.consumed.remove(&device);
    }
    Ok(())
}

fn refresh_devices(
    state: &mut InterceptionState,
    event_tx: &tokio::sync::mpsc::Sender<Result<KeyboardEvent, io::Error>>,
    keyboard_snapshot: &Arc<RwLock<Vec<Keyboard>>>,
    emit_changes: bool,
) -> io::Result<()> {
    let discovered = DeviceEnumerator::enumerate_keyboards(&state.context)?;
    let mut current = HashMap::new();
    for device in discovered {
        current.insert(device.handle.0, Arc::new(device.keyboard));
    }

    for (device, keyboard) in &current {
        if emit_changes && !state.keyboards.contains_key(device) {
            let _ = event_tx.blocking_send(Ok(KeyboardEvent::Plugged(Arc::clone(keyboard))));
        }
    }

    for (device, keyboard) in state.keyboards.drain() {
        if emit_changes && !current.contains_key(&device) {
            state.consumed.remove(&device);
            state.pressed_keys.retain(|(d, _)| d != &device);
            let _ = event_tx.blocking_send(Ok(KeyboardEvent::Unplugged(keyboard)));
        }
    }

    state.keyboards = current;
    if let Ok(mut snapshot) = keyboard_snapshot.write() {
        snapshot.clear();
        snapshot.extend(
            state
                .keyboards
                .values()
                .map(|keyboard| keyboard.as_ref().clone()),
        );
    }
    state.last_device_refresh = Instant::now();
    Ok(())
}

fn update_modifiers(
    modifiers: &mut keyboard_types::Modifiers,
    code: ScanCode,
    state: KeyState,
    repeat: bool,
) {
    let Some(modifier) = modifier_for_key(code, state) else {
        return;
    };
    let scan_code = code as u16;
    let is_lock = matches!(scan_code, 0x3A | 0x45 | 0x46);

    if state.contains(KeyState::UP) {
        if !is_lock {
            modifiers.remove(modifier);
        }
    } else if is_lock {
        if !repeat {
            modifiers.toggle(modifier);
        }
    } else {
        modifiers.insert(modifier);
    }
}
