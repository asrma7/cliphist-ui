# Changelog

## 0.1.1

- Added `SIGUSR1` live reload support for resident `--service` processes, re-reading config and CSS without restarting the app.
- Fixed user CSS import handling by loading `style.css` from its filesystem path, allowing relative `@import` files to resolve correctly.
- Reapply window, search, behavior, layer-shell, and CSS settings after a service config reload.
- Restart thumbnail workers when image settings or the thumbnail cache path changes, and ignore stale thumbnail results from previous worker generations.

## 0.1.0

- Initial tester release of `cliphist-ui`.
- Native GTK4 Wayland layer-shell popup for `cliphist`.
- Searchable clipboard history with keyboard and mouse navigation.
- Copy, delete, clear, service mode, JSON5 configuration, CSS styling, and lazy image thumbnails.
