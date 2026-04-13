# Sprout Mobile

Flutter mobile client for Sprout.

## Setup

```bash
cd mobile
flutter pub get
```

## Run

```bash
flutter run
```

## Test

```bash
flutter test
```

## Architecture

```
lib/
├── main.dart              # Entry point, Riverpod bootstrap
├── app.dart               # MaterialApp with theme
├── shared/
│   ├── theme/             # Catppuccin light/dark, spacing tokens, extensions
│   └── widgets/           # Shared widgets
└── features/
    └── home/              # Placeholder home surface
```

- **State management:** Riverpod + Hooks (`HookConsumerWidget`)
- **Theme:** Catppuccin Latte (light) / Macchiato (dark) — matches desktop
- **Spacing:** `Grid` tokens, no magic numbers
- **Feature isolation:** No cross-feature imports except `shared/`
