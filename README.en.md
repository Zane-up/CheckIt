# CheckIt

[简体中文](README.md) · English

Check it. Get it done. A lightweight Windows desktop task manager with a floating widget, countdowns and local reminders, built with Tauri 2, React, TypeScript, Rust and SQLite.

## Download

[Download the latest Windows installer](https://github.com/Zane-up/CheckIt/releases/latest).

Supports Windows 10/11, 64-bit. Under **Assets**, download the `.exe` installer and run it. Node.js and Rust are only needed for development. To upgrade, quit the previous version and install over the existing installation. Task data is stored separately from the application.

## Choose your language

Open **All tasks → Settings → 语言 / Language** and choose **English** or **简体中文**. The selection is saved locally and shared by the widget, manager, popup reminders, tray menu and system notification text. Upgrading from an older version defaults to Simplified Chinese.

Task names, descriptions and category names stay as you entered them. Dates use your local time zone; changing the interface language does not change due dates or reminder schedules.

## Features

- Create, edit, complete and categorize tasks with priorities and optional due dates.
- Keep a compact widget on your desktop, or drag it to an edge to dock as a circle showing your pending task count.
- Use preset or custom reminder lead times, system notifications, centered popup reminders, or both.
- Snooze currently displayed popup reminders for five or ten minutes. Snoozed and unacknowledged popup reminders survive restarts.
- Store tasks locally in SQLite. Task management and reminders work offline; checking for updates requires GitHub access.
- Choose dark, light or system theme. The manager has an optional glass effect; the widget uses an opaque surface for readable text and clean rounded corners.

Deleted tasks offer **Undo delete for ten seconds** and remain in **Recently deleted** for seven days. They stop reminding while deleted. Restoring retains task contents and future reminders without replaying past notifications. Expired entries are removed automatically.

## Backups and restores

Under **Settings → Local backup and restore**, select **Back up now**. **Open backup folder** lets you copy the resulting `.db` file to another drive. To restore an external backup, copy it back into that folder, refresh the list and select it.

Restoring replaces current tasks, categories, settings and reminder history, then restarts CheckIt. A safety backup is created before restoring and before database schema upgrades. If this backup fails, the operation is stopped. Backups are local and are not uploaded automatically.

**Daily automatic backup is enabled by default.** CheckIt backs up once per 24 hours while running and catches up once after restart or wake if overdue. Keep 3, 7, 14 or 30 automatic copies in Settings, view the last success or error, and retry immediately. Only recognized automatic backup files rotate; manual and safety copies stay. Keep an additional copy on a separate drive.

Reminder cards support completing a single task, snoozing it, or opening its details. Opening details acknowledges that reminder after requesting navigation. Any existing editor draft is kept until you close the editor, then navigation continues.

## Development

Requires Node.js 22.12 or later, stable Rust with the MSVC toolchain, and Visual Studio C++ Build Tools with a Windows SDK.

```sh
npm ci
npm run tauri dev
```

```sh
npm run typecheck
npm test
npm run tauri build
cd src-tauri
cargo test
cargo check --all-targets
```

[Release history](CHANGELOG.md) · [MIT license](LICENSE)
