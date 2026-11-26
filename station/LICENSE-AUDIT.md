# Проверка лицензий — 23 ноября 2026

Проверены фактические файлы коммита
`59dff1a36bb3a2308f120480e57a2e01c4ba1aae`, а не лицензия текущего main:

- `../LICENSE`: MIT, Copyright (c) 2026 Ludovic Marie and the Horizon contributors.
- `../Cargo.toml`: workspace.package.license = MIT.
- `../crates/horizon-core/Cargo.toml`: наследует MIT; единственная внешняя зависимость — libm.
- `../THIRD_PARTY_NOTICES.md` сохранён целиком. Он описывает исторические удаления
  несовместимых заимствований и отдельные условия сторонних компонентов. Наличие
  MIT в корне само по себе не доказывает происхождение каждой строки.
- Новый код station: MIT, `LICENSE` рядом. Спрайты SS14: CC BY-SA 3.0, отдельно
  от кода; [авторы](assets/ss14/ATTRIBUTION.md), [SHA-256](assets/ss14/MANIFEST.json).
- Rapier3D 0.22.0 и Parry3D: Apache-2.0; Macroquad 0.4.14: MIT.
  Полный список объявленных лицензий зафиксированных зависимостей, включая
  build/target-specific, сохранён в `DEPENDENCIES.md`. Unicode-3.0 также
  присутствует в выражении лицензии unicode-ident.

Код подключает horizon-core через path, без изменений его файлов. В графический
прототип не включены ресурсы Mojang, DXC DLL и подготовленные `.local` ресурсы.
Весь исходный checkout Horizon содержит и другие компоненты со своими лицензиями;
его нельзя считать целиком новым авторским кодом station.

Для распространения сохраняются исходные MIT-уведомления Horizon, а также
применимые тексты лицензий/NOTICE зависимостей. Этот инкремент содержит инвентаризацию
SPDX из Cargo metadata, но не полную экспертизу происхождения всех исходников
или готовый пакет лицензионных уведомлений для коммерческого бинарного релиза.
Более поздние изменения Horizon не включались.

Обновление графического инкремента: Macroquad больше не входит в зависимости.
Новый модуль station_renderer.rs написан для проекта и объявлен MIT через
station/render/Cargo.toml. wgpu — MIT OR Apache-2.0, winit — Apache-2.0;
актуальный полный список находится в DEPENDENCIES.md. Корневые vendor-патчи
Horizon этим workspace не используются; закреплённые registry-версии указаны
в station/Cargo.lock.

## SS14 inventory increment — 23 ноября 2026

- SS14 f5b48b6547a1f9bfd1f1613d879d68064e22f57f: исходники C#/XAML под MIT, копия уведомления и исходных файлов в ss14-port. Графические/звуковые ресурсы SS14 не копировались.
- rust-anything b30206b1af11c58706303dfe20a3bd2d819c1e68: MIT, копия лицензии в ss14-port. Сгенерированный мост служит проверке, игровая симуляция включает только чистый Rust port.rs.
- egui/egui-winit/egui-wgpu 0.37.0: MIT OR Apache-2.0 по Cargo metadata. Обновлён DEPENDENCIES.md.
- Geist-Regular.ttf переиспользован из Horizon, SIL OFL-1.1; исходное уведомление OFL-Geist.txt сохранено рядом с шрифтом.

Это проверка включённых файлов и объявленных лицензий, а не юридическое заключение или утверждение о совместимости всего контента SS14.

## Контейнеры: дополнительные исходники SS14

В reference добавлены BypassLockSystem.cs и SharedStorageSystem.cs из той же
MIT-ревизии f5b48b6547a1f9bfd1f1613d879d68064e22f57f. Уведомление Space Wizards
Federation сохранено. Новые изображения, звуки и зависимости не добавлялись.
Адаптированные проверки и FFI в ss14-storage-rules — MIT; FFI служит только
проверке C#↔Rust и не используется игровым процессом.

## Проверки DoAfter

SharedDoAfterSystem.Update.cs и DoAfterArgs.cs сохранены из той же MIT-ревизии
SS14. Адаптер action-oracle/Rules.cs и Rust port.rs сохраняют уведомление Space
Wizards Federation. Мост ss14-action-rules используется для проверки; новые
ресурсы и зависимости игрового workspace не добавлены.

### Lighting increment

`ss14-light-rules` ports the scalar attenuation calculation from RobustToolbox `LightLevelSystem.GetColourFromLight`, introduced by 5e79016217f5c606a2a21ad54750ebb843d1343f (2026-07-23). This is post-2019 MIT code; notice in `ss14-port/ROBUST-LICENSE.txt`. Preset CurveFactor=1/Falloff=1, original float operations retained. Voxel visibility and renderer integration are original station code. See LIGHTING.md. No asset imports.

Добавлены 11 RSI-наборов SS14 из ревизии f5b48b6547a1f9bfd1f1613d879d68064e22f57f.
Оригинальные PNG и meta.json сохранены без изменений. Алгоритм RSI FoldDelays
перенесён из MIT-кода RobustToolbox; подробности в [SPRITES.md](SPRITES.md).

Перенос AdvanceFrameAnimation: MIT-код RobustToolbox 2023–2025, история строк
и неизменённый C#-эталон сохранены; [проверка происхождения](ss14-port/PLAYBACK.md).
Схема Station, GPU-интеграция и составные UI-иконки используют прежнюю MIT-лицензию кода.
