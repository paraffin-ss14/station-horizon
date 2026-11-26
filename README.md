# Station — самостоятельный прототип на Rust

Текущая игра находится в [station](station/README.md): камера строго сверху,
воксельный мир, Rapier3D, руки и инвентарь, контейнеры, инструменты и длительные действия.

Из корня репозитория:

```powershell
cargo run --manifest-path station/Cargo.toml -p station-client --locked
cargo test --manifest-path station/Cargo.toml --workspace --locked
```

[Управление и устройство](station/README.md) · [Результаты проверок](station/VERIFICATION.md)

## Основа и лицензии

Исходный код Horizon сохранён на базе MIT-ревизии
`59dff1a36bb3a2308f120480e57a2e01c4ba1aae`. В `crates`, `launcher`, `tools` и `docs`
остаётся код и техническая документация исходного Minecraft-совместимого проекта:
они не означают, что все эти системы используются станцией.

Скриншоты Minecraft, старые рекламные изображения и ненужные тестовые датапаки
удалены из рабочей копии. Станция не требует Minecraft JAR или ресурсов Mojang.
Сохранены общие шрифты, элементы управления, иконки сборки и все лицензионные
уведомления. Подробнее: [отчёт очистки](station/CLEANUP.md).

[LICENSE](LICENSE) · [THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES.md) ·
[проверка лицензий станции](station/LICENSE-AUDIT.md)
