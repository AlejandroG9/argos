# Argos — instrucciones para agentes

Este archivo es la fuente única de reglas del proyecto. `CLAUDE.md` y `GEMINI.md`
apuntan aquí.

## Qué es Argos

Una app de escritorio nativa en Rust que monitorea agentes de IA (Claude Code, Codex,
Gemini, Antigravity) ejecutados como CLIs dentro de la terminal Warp, a través de todos
los proyectos del usuario. Muestra qué agente de qué compañía trabaja en qué rama y si
está trabajando, terminó o espera respuesta del usuario.

No instrumenta a los agentes: lee los archivos de sesión que cada CLI ya escribe en disco,
la tabla de procesos del sistema y el estado de git.

## Lee esto antes de escribir código

En este orden, completos:

1. `docs/superpowers/specs/2026-09-26-argos-design.md` — el diseño. Contiene los hallazgos
   empíricos sobre los formatos en disco de cada plataforma. **Verificarlos costó trabajo:
   no los re-investigues, confía en lo documentado ahí.**
2. `docs/superpowers/plans/2026-09-26-argos.md` — el plan, en 14 tareas numeradas.

El plan tiene código real en cada paso. No es pseudocódigo ni sugerencias: escríbelo tal
como está salvo que no compile, y si no compila arregla lo mínimo y anótalo en el commit.

## Cómo tomar trabajo

Toma **una tarea completa** del plan, de principio a fin, y no empieces otra hasta
cerrarla. Cada tarea termina con un entregable que se puede probar por sí solo.

Marca las casillas `- [ ]` del plan a medida que completas cada paso.

### Trabajo en paralelo: un worktree por tarea

Varios agentes de plataformas distintas trabajan en este repo al mismo tiempo. Las tareas
3 a 8 son independientes entre sí. **Para no pisar a nadie, trabaja en tu propio worktree:**

```bash
cd ~/Proyectos/argos
git worktree add .worktrees/tarea-N -b feat/tarea-N-nombre-corto
cd .worktrees/tarea-N
```

Commitea ahí. No hagas merge a `main` tú mismo ni toques la rama de otro agente: al
terminar, reporta que la rama está lista y el usuario integra.

Las tareas 1, 2 y de la 9 en adelante son secuenciales y sí van sobre `main`, porque cada
una depende de la anterior.

## Reglas no negociables

- **Rust 1.91.1 stable**, edición 2024, target `aarch64-apple-darwin`.
- **Solo macOS.** Se depende de `ps`, `lsof`, `open` y de las variables `WARP_*`. No
  escribas rutas de compatibilidad para Linux o Windows: son código muerto.
- **Argos es de solo lectura sobre los datos de los agentes.** Nunca escribas en los
  archivos de sesión de otro CLI ni mandes input a sus procesos. La única escritura
  permitida es la propia base SQLite de Argos.
- **Nunca `unwrap()` ni `expect()` sobre datos externos** — contenido de archivos, salida
  de comandos, entorno de procesos. Los formatos son privados y no documentados, y pueden
  cambiar sin aviso en cualquier actualización. En tests sí se permite.
- **Un probe que falla degrada solo su plataforma.** Nunca propagues un error que tumbe el
  ciclo de sondeo o afecte a otros probes.
- **SQLite es un índice derivado, no la fuente de verdad.** Todo lo que se guarda debe
  poder reconstruirse desde los archivos de sesión. Si añades una columna, asegúrate de que
  el reindexado la repuebla.
- **Métricas opcionales por plataforma.** No obligues a que todas expongan tokens o
  subagentes. Claude Code da tokens; Antigravity no. Ambos casos son válidos.
- **Añadir una plataforma = añadir un probe.** No metas `match` sobre `ClientKind` fuera de
  `model.rs` y del registro de probes.
- **Usa `cargo add`, no versiones escritas a mano.** No inventes números de versión de
  crates.

## Ciclo de trabajo (TDD, obligatorio)

Cada tarea del plan ya viene partida en este ciclo. Síguelo en orden y **no te adelantes**:

1. Escribe el test que falla.
2. Córrelo y **confirma que falla**, por el motivo esperado. Un test que pasa antes de
   existir la implementación no está probando nada.
3. Escribe la implementación mínima que lo hace pasar.
4. Córrelo y confirma que pasa.
5. Commitea.

No escribas implementación antes del test. No escribas más implementación de la que el
test exige.

## Verificación

Antes de dar una tarea por terminada:

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

Los tres tienen que pasar limpio. `clippy` con `-D warnings` significa que una advertencia
es un fallo.

Para comprobar el núcleo completo contra los datos reales de la máquina, sin GUI:

```bash
cargo run -p argos-core --bin argos-probe
```

Debe imprimir las sesiones reales del usuario. Compara con `git worktree list` en algún
repo y con las panes de Warp abiertas: si una rama o un estado no cuadra, hay un bug.

**No afirmes que algo funciona sin haber corrido el comando y visto la salida.** Pega la
salida real en tu reporte, no un resumen de lo que esperabas.

## Commits

Mensajes en español, con prefijo convencional: `feat:`, `fix:`, `test:`, `docs:`,
`refactor:`. Una línea de asunto que diga el *por qué* cuando no sea obvio.

Commits frecuentes: uno por paso del plan que lo pida, no uno gigante al final.

## Estilo de código

- **Por defecto, cero comentarios.** Escribe un comentario solo cuando el *por qué* no sea
  evidente: una restricción oculta, una decisión contraintuitiva, un formato externo raro.
  Nunca comentes *qué* hace el código.
- Los comentarios que **sí** valen la pena en este proyecto son los que explican los
  formatos ajenos, porque no están documentados en ningún lado. Ejemplo legítimo: "el
  timestamp de Antigravity viene en milisegundos, a diferencia del resto".
- Nada de comentarios `// TODO`, `// añadido para X`, `// eliminado`.
- Nombres en inglés para el código (tipos, funciones, campos); texto en español para
  mensajes de usuario, comentarios y tests.
- Archivos enfocados y pequeños. Si un archivo crece demasiado, está haciendo demasiado.

## Qué no hacer

- No refactorices más allá de lo que tu tarea pide. Un arreglo de bug no necesita limpieza
  de los alrededores.
- No añadas manejo de errores para escenarios imposibles, ni banderas de configuración, ni
  capas de compatibilidad "por si acaso".
- No añadas dependencias que el plan no pida. Si crees que hace falta una, dilo antes de
  meterla.
- No cambies el spec ni el plan por tu cuenta. Si encuentras un error en ellos, para y
  repórtalo: puede afectar a las tareas de otros agentes.
- No implementes cosas marcadas como fuera de alcance de v1: control de agentes (responder,
  pausar, matar), grafos de nodos, animaciones, timelines, monitoreo remoto.

## Si te bloqueas

Para y reporta. Di exactamente qué intentaste, qué salida obtuviste y qué decisión
necesitas. Dos casos concretos que el plan anticipa:

- **Tareas 5 y 6** (probes de Codex y Gemini): el formato interno de los turnos no está
  verificado. El primer paso de esas tareas es un comando de inspección. El plan define un
  contrato obligatorio con dos resultados aceptables: si el formato permite detectar
  llamadas a herramienta pendientes, úsalo; si no, devuelve `Indeterminate` y declara
  `tool_level_detail: false`. Cualquiera de los dos está bien — documenta cuál encontraste.
- Si un formato en disco ya no coincide con lo documentado en el spec, es que la plataforma
  cambió. No improvises: repórtalo, porque hay que actualizar el spec.
