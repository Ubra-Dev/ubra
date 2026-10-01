//! Keep persisted window geometry usable when displays change or stale state is restored.

use tauri::{PhysicalPosition, PhysicalSize, Runtime, Window};

const DISPLAY_MARGIN: u32 = 24;

/// Clamp the main window to the display it currently overlaps, or the primary display
/// when its saved position no longer intersects any connected display.
pub fn keep_main_window_on_screen<R: Runtime>(window: &Window<R>) -> tauri::Result<()> {
    if window.is_maximized()? || window.is_fullscreen()? || window.is_minimized()? {
        return Ok(());
    }

    let monitors = window.available_monitors()?;
    if monitors.is_empty() {
        return Ok(());
    }

    let position = window.outer_position()?;
    let inner_size = window.inner_size()?;
    let outer_size = window.outer_size()?;

    let monitor = monitors
        .iter()
        .max_by_key(|monitor| overlap_area(position, outer_size, monitor.work_area()))
        .filter(|monitor| overlap_area(position, outer_size, monitor.work_area()) > 0)
        .cloned()
        .or(window.primary_monitor()?)
        .or_else(|| monitors.first().cloned());
    let Some(monitor) = monitor else {
        return Ok(());
    };

    let work_area = monitor.work_area();
    let margin_x = DISPLAY_MARGIN.min(work_area.size.width / 2);
    let margin_y = DISPLAY_MARGIN.min(work_area.size.height / 2);
    let max_outer_width = work_area.size.width.saturating_sub(margin_x * 2);
    let max_outer_height = work_area.size.height.saturating_sub(margin_y * 2);
    let chrome_width = outer_size.width.saturating_sub(inner_size.width);
    let chrome_height = outer_size.height.saturating_sub(inner_size.height);
    let bounded_size = PhysicalSize {
        width: inner_size
            .width
            .min(max_outer_width.saturating_sub(chrome_width)),
        height: inner_size
            .height
            .min(max_outer_height.saturating_sub(chrome_height)),
    };
    let bounded_outer_width = bounded_size
        .width
        .saturating_add(chrome_width)
        .min(max_outer_width);
    let bounded_outer_height = bounded_size
        .height
        .saturating_add(chrome_height)
        .min(max_outer_height);

    if bounded_size != inner_size {
        window.set_size(bounded_size)?;
    }

    let min_x = i64::from(work_area.position.x) + i64::from(margin_x);
    let min_y = i64::from(work_area.position.y) + i64::from(margin_y);
    let max_x = i64::from(work_area.position.x) + i64::from(work_area.size.width)
        - i64::from(margin_x)
        - i64::from(bounded_outer_width);
    let max_y = i64::from(work_area.position.y) + i64::from(work_area.size.height)
        - i64::from(margin_y)
        - i64::from(bounded_outer_height);
    let bounded_position = PhysicalPosition {
        x: i64::from(position.x).clamp(min_x, max_x) as i32,
        y: i64::from(position.y).clamp(min_y, max_y) as i32,
    };

    if bounded_position != position {
        window.set_position(bounded_position)?;
    }

    Ok(())
}

fn overlap_area(
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    work_area: &tauri::PhysicalRect<i32, u32>,
) -> u64 {
    let left = i64::from(position.x).max(i64::from(work_area.position.x));
    let top = i64::from(position.y).max(i64::from(work_area.position.y));
    let right = (i64::from(position.x) + i64::from(size.width))
        .min(i64::from(work_area.position.x) + i64::from(work_area.size.width));
    let bottom = (i64::from(position.y) + i64::from(size.height))
        .min(i64::from(work_area.position.y) + i64::from(work_area.size.height));

    if right <= left || bottom <= top {
        return 0;
    }

    ((right - left) as u64) * ((bottom - top) as u64)
}
