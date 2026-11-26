use std::sync::{Mutex, mpsc};
use std::thread::{self, JoinHandle};
use tauri::{PhysicalPosition, PhysicalSize, WebviewWindow};

type Point = (i32, i32);

pub struct PositionWriter(Mutex<Option<(mpsc::Sender<Point>, JoinHandle<()>)>>);

impl PositionWriter {
    pub fn start(mut persist: impl FnMut(i32, i32) + Send + 'static) -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("widget-position-writer".into())
            .spawn(move || {
                for (x, y) in receiver {
                    persist(x, y);
                }
            })?;
        Ok(Self(Mutex::new(Some((sender, worker)))))
    }

    pub fn queue(&self, point: Point) -> Result<(), String> {
        let state = self.0.lock().map_err(|_| "position writer lock poisoned")?;
        let (sender, _) = state.as_ref().ok_or("position writer stopped")?;
        sender.send(point).map_err(|e| e.to_string())
    }

    pub fn flush_and_join(&self) {
        let state = self.0.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some((sender, worker)) = state {
            // Closing the only sender lets the worker drain the queue in order before exiting.
            drop(sender);
            if worker.join().is_err() {
                eprintln!("widget position writer stopped unexpectedly");
            }
        }
    }
}

fn point_is_on_monitor(
    (x, y): Point,
    monitors: impl IntoIterator<Item = (PhysicalPosition<i32>, PhysicalSize<u32>)>,
) -> bool {
    monitors.into_iter().any(|(origin, size)| {
        let dx = i64::from(x) - i64::from(origin.x);
        let dy = i64::from(y) - i64::from(origin.y);
        (0..i64::from(size.width)).contains(&dx) && (0..i64::from(size.height)).contains(&dy)
    })
}

pub fn restore(window: &WebviewWindow, saved: Option<Point>) -> tauri::Result<()> {
    if let Some((x, y)) = saved {
        let monitors = window.available_monitors()?;
        if point_is_on_monitor((x, y), monitors.iter().map(|m| (*m.position(), *m.size()))) {
            window.set_position(PhysicalPosition::new(x, y))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn shutdown_drains_all_positions_in_order_and_stops_accepting_moves() {
        let written = Arc::new(Mutex::new(Vec::new()));
        let output = Arc::clone(&written);
        let writer =
            PositionWriter::start(move |x, y| output.lock().unwrap().push((x, y))).unwrap();
        for point in [(10, 20), (30, 40), (-50, 60)] {
            writer.queue(point).unwrap();
        }
        writer.flush_and_join();
        assert_eq!(*written.lock().unwrap(), [(10, 20), (30, 40), (-50, 60)]);
        assert!(writer.queue((4, 5)).is_err());
        writer.flush_and_join();
    }

    #[test]
    fn restore_classifies_negative_origins_gaps_and_exclusive_edges() {
        let monitors = [
            (PhysicalPosition::new(0, 0), PhysicalSize::new(1920, 1080)),
            (
                PhysicalPosition::new(-1440, -900),
                PhysicalSize::new(1440, 900),
            ),
        ];
        for point in [(-1440, -900), (-1, -1), (0, 0), (1919, 1079)] {
            assert!(point_is_on_monitor(point, monitors), "{point:?}");
        }
        for point in [
            (1920, 0),
            (0, 1080),
            (-1, 0),
            (0, -1),
            (-1441, -900),
            (i32::MIN, 0),
        ] {
            assert!(!point_is_on_monitor(point, monitors), "{point:?}");
        }
        assert!(!point_is_on_monitor((0, 0), []));
        assert!(point_is_on_monitor(
            (i32::MAX, i32::MAX),
            [(
                PhysicalPosition::new(i32::MAX, i32::MAX),
                PhysicalSize::new(1, 1)
            )],
        ));
    }
}
