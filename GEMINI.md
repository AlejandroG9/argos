# Argos — Gemini / Antigravity

**Lee `AGENTS.md` completo antes de tocar nada.** Es la fuente única de reglas del
proyecto: qué es Argos, qué leer primero, el ciclo TDD obligatorio, las reglas no
negociables y los comandos de verificación. Este archivo solo añade lo específico de tu
plataforma.

## Resumen en dos líneas

Argos es una app de escritorio en Rust que monitorea agentes de IA corriendo como CLIs en
Warp, mostrando qué agente trabaja en qué rama y en qué estado. El trabajo está planificado
en 14 tareas en `docs/superpowers/plans/2026-09-26-argos.md`.

## Específico de Antigravity (`agy`)

Eres además una de las plataformas que Argos monitorea. La **Tarea 7** implementa tu propio
probe, y lo que hay documentado sobre tu formato en disco salió de inspeccionar tus archivos
reales:

- Tu historial vive en `~/.gemini/antigravity-cli/history.jsonl`, un archivo **global** (no
  uno por proyecto), con una línea por prompt: `{"display", "timestamp", "workspace"}`.
- Tu `timestamp` es **epoch en milisegundos**, a diferencia de Claude Code y Codex que usan
  ISO-8601. Confundir las unidades da fechas en 1970. Hay un test que lo fija.
- Como el archivo es global, hay que agrupar por `workspace`: cada workspace distinto es
  una sesión lógica.
- Tu formato no expone detalle a nivel de herramienta ni conteo de tokens, así que tu probe
  declara `Capabilities::minimal()` y devuelve `ActivitySemantics::Indeterminate`. Eso es
  correcto y deliberado, no una carencia que haya que rellenar.

Si al implementar encuentras que tu CLI sí deja información más rica en otro lado
(`conversation_summaries.db` es SQLite, `antigravity_state.pbtxt` es protobuf), **no lo
metas en la Tarea 7**. Repórtalo: parsear eso es trabajo posterior y aislado, y el spec lo
tiene contemplado así a propósito.

## Recordatorio de disciplina

Otros agentes de otras plataformas trabajan en este repo al mismo tiempo. Si tu tarea está
entre la 3 y la 8, trabaja en tu propio worktree como indica `AGENTS.md`, y no hagas merge
a `main`.
