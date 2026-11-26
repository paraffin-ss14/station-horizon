# Исследование предоставленного Iris JAR

Исследован локальный `C:/Users/1/Downloads/iris-neoforge-1.11.7-mc26.3.jar`.
SHA256: `2260506334A17AC566EA30169AB491201208F55C1C7212BB987BC0C7C59A8D93`.
Дата: 24 ноября 2026. Метод: чтение ZIP, метаданных, структуры class-файлов,
полей, сигнатур, constant pool и короткого bytecode BufferFlipper; сопоставление
с Rust-исходниками Horizon. JAR не запускался и не изменялся. Это статическое
исследование архитектуры, не полная декомпиляция и не проверка поведения в Minecraft.
Происхождение сборки по внешней подписи/официальному хешу не проверялось.

## Что находится в файле

1186 ZIP-записей. В META-INF/neoforge.mods.toml указаны modId iris,
версия 1.11.7+mc26.3 и лицензия LGPL-3.0-only. Это заявленная лицензия модуля,
она отличается от MIT нашей station/render. Заявлены зависимости Minecraft,
NeoForge и Sodium, несовместимость с Embeddium. В metadata диапазон Minecraft
`[1.21.3,)`, несмотря на mc26.3 в строке версии; одного этого диапазона недостаточно
для вывода о поддержке старых версий. Изученные class-файлы имеют major 69
(Java 25). Manifest указывает Main-Class LaunchWarn; это мод, не автономный
шейдерный движок, который можно непосредственно загрузить в Rust/wgpu.

В ссылках классов видны Minecraft, RenderPearl/OpenGL, GLSL-transformer,
Sodium/Distant Horizons и Java-библиотеки. Прямое подключение JAR потребовало
бы среды JVM и соответствующих API игры; наш renderer работает через wgpu.

## Подтверждённые механизмы

| Область | Что обнаружено в JAR | Значение для станции |
|---|---|---|
| Описание программ | ProgramSet хранит gbufferPrograms, compositePrograms, computePrograms, setup, shadowCompute, finalCompute | Нельзя считать загрузку final поддержкой всего пакета |
| Исполнение проходов | CompositeRenderer содержит renderAll, createComputes, setupMipmapping, размеры, viewportScale, drawBuffers, blendModeOverride; ссылки на dispatch и memoryBarrier | Нужны порядок GPU-проходов, размеры, blend, compute и синхронизация |
| Переключение буферов | BufferFlipper хранит множество индексов; bytecode flip удаляет индекс, а если его нет — добавляет; есть snapshot | У каждого colortex есть состояние main/alt, которое определяет вход следующего прохода |
| Директивы | ProgramDirectives: drawBuffers, mipmappedBuffers, explicitFlips, bufferBlendInformations; ShaderProperties: условные программы, размеры, изображения, SSBO, custom textures/uniforms | Это исполняемые правила, а не справочные настройки |
| Глубина | RenderTargets содержит currentDepthTexture, noTranslucents, noHand; методы copyPreTranslucentDepth и copyPreHandDepth | Одну глубину нельзя считать универсальной заменой всех depthtex |
| Кадровые данные | MatrixUniforms связан с CapturedRenderingState и отдельными Previous/Inverted; CustomUniforms имеет зависимости, update и push | Для temporal-эффектов нужны настоящие предыдущие матрицы и значения выражений |
| GLSL | TransformPatcher имеет patchComposite, patchCompute, patchSodium, patchVanilla и AST transformer | Важна совместимость языка и интерфейсов шейдеров, не только компиляция отдельных файлов |
| Фазы мира | IrisRenderingPipeline: beginLevelRendering, renderShadows, beginHand, beginTranslucents, finalizeLevelRendering, finalizeGameRendering | Прозрачность и тени должны исполняться в согласованных фазах сцены |

Наличие полей/ссылок подтверждено непосредственно бинарником. Точные условия
ветвления большого renderAll и полная последовательность всех фаз не были
доказаны одним анализом constant pool; для них нужны дальнейшие targeted probes.

## Существенное открытие в нашем репозитории

В Horizon уже есть не только GLSL-загрузчик, но и значительно более общий
исполнитель: `crates/horizon-client-render/src/gfx/pack_post.rs`.
Он описывает A/B буферы, перенос неочищаемой истории между кадрами и GL-порядок
строк; в рабочем цикле есть setup/begin/prepare, deferred_pre/composite_pre,
compute dispatch, генерация недействительных mip-уровней, viewport и выводы.
Соседние pack_uniforms, pack_images, pack_textures и pack_draw обслуживают
ресурсы и геометрию. В коде уже есть комментарии и тесты со ссылками на
наблюдаемое поведение Iris 1.11.7. Это существующая база, не результат данного
исследования и не доказательство полной совместимости нашей станции.

`crates/horizon-shaders/src/post.rs` уже хранит в PostProgram: buffers,
flips, blend, buffer_blends, mipmaps, compute, scale. Станционный
`station_shader_pipeline.rs` загружает эту модель и проверяет возможности,
но не исполняет её. `station/render/Cargo.toml` использует station_renderer.rs
как отдельный корень библиотеки; общий gfx/pack_post не подключается автоматически.
Текущие station_shader_pack, station_photon_* и station_solas_post исполняют
конкретные адаптированные подмножества. Поэтому успешный shader probe или
наличие общего исполнителя в исходниках ещё не означает, что он работает в станции.

## Вывод для дальнейшей реализации

Следующий архитектурный шаг — выделить/адаптировать общий исполнитель из
существующей инфраструктуры Horizon, используя Iris как эталон поведения,
вместо дальнейшего ручного перечисления composite13/14/15 для каждого пакета.
Приоритеты:

1. Реальные A/B colortex, DRAWBUFFERS/RENDERTARGETS, flip, clear и сохранение истории.
2. Исполнение списка PostProgram с размерами, viewport, blend и нужными mipmaps.
3. Согласованные depthtex, матрицы текущего/предыдущего кадра, material/gbuffer ABI станции.
4. Compute, SSBO/images, custom textures и выражения uniforms.
5. Сравнительные GPU-пакеты для порядка проходов, пропущенных программ, resize и temporal reset.

Общий исполнитель также нельзя просто вставить одной строкой: он связан с
геометрией, материалами, sky/water и кадровыми данными обычного Horizon.
Сначала нужен явный контракт сцены станции. В этой задаче код игры не менялся,
сборка/тесты повторно не запускались: выполнено исследование файла и архитектуры.


## Последующее внедрение

Первый этап выполнен после этого исследования: общий A/B colortex bank
подключён к Solas, с shared-парсером flip из Horizon и чтением Clear/ClearColor.
Подробности, GPU-проверки и оставшиеся ограничения — в SHADER-PACKS.md,
раздел «Общий слой A/B colortex». Общий планировщик PostProgram ещё не подключён.

Следующее внедрение: выбранные Solas composite13/14/15 теперь загружаются
через общий Loader Horizon как PostProgram (load_post_subset) и реально
исполняются станционным адаптером. Это заменяет прежний отдельный разбор
этих проходов; полный граф пакета и compute по-прежнему не исполняются.
