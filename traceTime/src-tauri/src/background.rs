use crate::tracker::{
    BackgroundMode, DEFAULT_BACKGROUND_OPACITY, effective_background_mode, parse_background_opacity,
};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{Emitter, Manager, WebviewWindow};

type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundModeResult {
    pub requested: BackgroundMode,
    pub effective: BackgroundMode,
    pub background_opacity: u8,
}

// Retain the exact effect so clearing it does not also try unsupported platform APIs.
#[derive(Clone, Copy)]
enum NativeGlass {
    #[cfg(target_os = "macos")]
    Vibrancy,
    #[cfg(windows)]
    Mica,
    #[cfg(windows)]
    Acrylic,
}

impl NativeGlass {
    fn apply(_window: &WebviewWindow) -> Option<Self> {
        #[cfg(target_os = "macos")]
        if window_vibrancy::apply_vibrancy(
            _window,
            window_vibrancy::NSVisualEffectMaterial::HudWindow,
            Some(window_vibrancy::NSVisualEffectState::Active),
            Some(8.0),
        )
        .is_ok()
        {
            return Some(Self::Vibrancy);
        }
        #[cfg(windows)]
        {
            if window_vibrancy::apply_mica(_window, Some(true)).is_ok() {
                return Some(Self::Mica);
            }
            if window_vibrancy::apply_acrylic(_window, Some((24, 24, 28, 190))).is_ok() {
                return Some(Self::Acrylic);
            }
        }
        // Linux blur is compositor-controlled; there is no portable native effect.
        None
    }

    fn clear(self, _window: &WebviewWindow) -> Result<()> {
        match self {
            #[cfg(target_os = "macos")]
            Self::Vibrancy => window_vibrancy::clear_vibrancy(_window)
                .map(|_| ())
                .map_err(|e| e.to_string()),
            #[cfg(windows)]
            Self::Mica => window_vibrancy::clear_mica(_window).map_err(|e| e.to_string()),
            #[cfg(windows)]
            Self::Acrylic => window_vibrancy::clear_acrylic(_window).map_err(|e| e.to_string()),
        }
    }
}

struct BackgroundState {
    mode: BackgroundModeResult,
    native: Option<NativeGlass>,
    faded: bool,
}

impl BackgroundState {
    fn ensure_glass(&mut self, window: &WebviewWindow) {
        if self.native.is_none() {
            self.native = NativeGlass::apply(window);
        }
        self.mode.effective = effective_background_mode(self.mode.requested, self.native.is_some());
    }

    fn clear_glass(&mut self, window: &WebviewWindow) -> Result<()> {
        if let Some(effect) = self.native {
            effect.clear(window)?;
            self.native = None;
        }
        Ok(())
    }

    fn set_mode(
        &mut self,
        window: &WebviewWindow,
        requested: BackgroundMode,
        background_opacity: u8,
    ) -> Result<BackgroundModeResult> {
        self.mode.background_opacity = background_opacity;
        if requested == BackgroundMode::Glass {
            self.mode.requested = requested;
            if self.mode.effective != BackgroundMode::Glass {
                // Probe newly requested glass even when idle so the settings UI gets the
                // real effective mode. Suspend the effect again before returning to the event loop.
                self.ensure_glass(window);
            }
            if self.faded {
                self.clear_glass(window)?;
            }
        } else {
            self.clear_glass(window)?;
            self.mode = BackgroundModeResult {
                requested,
                effective: requested,
                background_opacity,
            };
        }
        Ok(self.mode)
    }

    fn set_faded(&mut self, window: &WebviewWindow, faded: bool) -> Result<()> {
        self.faded = faded;
        if faded {
            self.clear_glass(window)?;
        } else if self.mode.requested == BackgroundMode::Glass
            && self.mode.effective == BackgroundMode::Glass
        {
            self.ensure_glass(window);
        }
        Ok(())
    }
}

struct WidgetBackground(Mutex<BackgroundState>);

pub fn initialize_widget_background(
    app: &tauri::App,
    window: &WebviewWindow,
    requested: BackgroundMode,
    background_opacity: u8,
) {
    let mut state = BackgroundState {
        mode: BackgroundModeResult {
            requested: BackgroundMode::Translucent,
            effective: BackgroundMode::Translucent,
            background_opacity: DEFAULT_BACKGROUND_OPACITY,
        },
        native: None,
        faded: false,
    };
    if let Err(error) = state.set_mode(window, requested, background_opacity) {
        eprintln!("cannot apply widget background: {error}");
    }
    app.manage(WidgetBackground(Mutex::new(state)));
}

// All writes to the main window's native material run through this path on the UI thread.
async fn update_widget_background<T: Send + 'static>(
    app: tauri::AppHandle,
    update: impl FnOnce(&mut BackgroundState, &WebviewWindow) -> Result<T> + Send + 'static,
) -> Result<T> {
    let window = app
        .get_webview_window("main")
        .ok_or("main window not found")?;
    let native_window = window.clone();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    window
        .run_on_main_thread(move || {
            let Some(state) = native_window.try_state::<WidgetBackground>() else {
                let _ = sender.send(Err("widget background is not initialized".into()));
                return;
            };
            let result = state
                .0
                .lock()
                .map_err(|_| "widget background lock poisoned".to_string())
                .and_then(|mut state| {
                    let previous = state.mode;
                    let result = update(&mut state, &native_window);
                    if state.mode != previous {
                        let _ =
                            native_window.emit_to("main", "background-mode-changed", state.mode);
                    }
                    result
                });
            let _ = sender.send(result);
        })
        .map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || receiver.recv().map_err(|e| e.to_string())?)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn get_widget_background_mode(app: tauri::AppHandle) -> Result<BackgroundModeResult> {
    let state = app
        .try_state::<WidgetBackground>()
        .ok_or("widget background is not initialized")?;
    let state = state
        .0
        .lock()
        .map_err(|_| "widget background lock poisoned")?;
    Ok(state.mode)
}

#[tauri::command]
pub async fn apply_background_mode(
    app: tauri::AppHandle,
    mode: String,
    opacity: u8,
) -> Result<BackgroundModeResult> {
    let requested = BackgroundMode::parse(&mode)?;
    let background_opacity = parse_background_opacity(&opacity.to_string())?;
    update_widget_background(app, move |state, window| {
        state.set_mode(window, requested, background_opacity)
    })
    .await
}

#[tauri::command]
pub async fn set_widget_idle_fade(window: WebviewWindow, faded: bool) -> Result<()> {
    if window.label() != "main" {
        return Err("idle fade is only available for the main widget".into());
    }
    update_widget_background(window.app_handle().clone(), move |state, window| {
        state.set_faded(window, faded)
    })
    .await
}
