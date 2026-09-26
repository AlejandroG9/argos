# Argos — Claude Code

**Lee `AGENTS.md` completo antes de tocar nada.** Es la fuente única de reglas del
proyecto: qué es Argos, qué leer primero, el ciclo TDD obligatorio, las reglas no
negociables y los comandos de verificación. Este archivo solo añade lo específico de esta
plataforma.

## Resumen en dos líneas

Argos es una app de escritorio en Rust que monitorea agentes de IA corriendo como CLIs en
Warp, mostrando qué agente trabaja en qué rama y en qué estado. El trabajo está planificado
en 14 tareas en `docs/superpowers/plans/2026-09-26-argos.md`.

## Específico de Claude Code

Eres además una de las plataformas que Argos monitorea, y la que deja los datos más ricos.
La **Tarea 4** implementa tu probe, con el formato ya verificado contra archivos reales:

- Tus sesiones viven en `~/.claude/projects/<slug>/<uuid>.jsonl`, y tus **subagentes** en
  `<slug>/<sesión-uuid>/subagents/agent-<id>.jsonl` — la relación padre-hijo va codificada
  en la ruta.
- Tus entradas traen `cwd` y `gitBranch` directamente, y `message.usage` con el desglose de
  tokens. Eres la única plataforma con las tres capacidades completas.
- **No mantienes abierto el archivo de sesión** (se verificó: cero descriptores `.jsonl`
  abiertos en un proceso vivo). Por eso la correlación proceso↔sesión es heurística y lleva
  un nivel de confianza explícito. No intentes "arreglarlo" con `lsof`: ya se descartó.

## Ejecución del plan

Si vas a implementar varias tareas en esta sesión, usa el skill
`superpowers:subagent-driven-development` o `superpowers:executing-plans`, no improvises el
orden. El plan ya declara qué tareas son paralelas (3 a 8) y qué tareas son secuenciales.

Antes de dar cualquier cosa por terminada, aplica
`superpowers:verification-before-completion`: corre los comandos y pega la salida real. En
este proyecto es especialmente importante, porque casi todo depende de formatos externos que
pueden haber cambiado desde que se escribió el spec.
