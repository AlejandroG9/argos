# Argos — Diseño

**Fecha:** 2026-09-26
**Estado:** Aprobado para planificación

## 1. Propósito

Argos es una aplicación de escritorio nativa que monitorea agentes de IA ejecutados como
CLIs, a través de todos los proyectos del usuario. Responde de un vistazo a la pregunta:
*¿qué agente, de qué compañía, está trabajando en qué rama, y en qué estado está?*

El usuario ejecuta simultáneamente varias sesiones de distintos agentes (Claude Code,
Codex, Gemini, Antigravity) dentro de la terminal Warp. Algunas trabajan sobre `main`
para cambios rápidos; otras sobre git worktrees aislados para trabajo que requiere
planificación. Hoy no existe forma de ver ese panorama completo: hay que recorrer panes
de terminal una por una.

### Objetivos

1. Ver en vivo el estado de todos los agentes activos, agrupados por proyecto y rama.
2. Distinguir tres estados que importan operativamente: **trabajando**, **terminó**,
   **esperando respuesta del usuario**.
3. Saltar desde el tablero a la sesión de terminal correspondiente.
4. Persistir lo observado para análisis histórico y comparación de rendimiento entre
   plataformas.

### No objetivos (v1)

- Controlar agentes (responder, pausar, matar). Se contempla para una fase posterior y
  el diseño reserva la costura, pero v1 es de solo lectura.
- Grafos de nodos, animaciones o timelines. Ver §8.
- Monitoreo remoto o multi-máquina. Todo es local.
- Pestañas por proyecto y desprendimiento a ventana propia. Acordado con el usuario para
  una fase posterior, después de §13: construirlas sobre un sondeo que congela la ventana
  multiplicaría el problema por pestaña en vez de resolverlo.

### Criterios de éxito

- Con varias sesiones concurrentes en curso, el tablero refleja la realidad sin que el
  usuario tenga que tocar ninguna terminal.
- El estado "esperando respuesta" es confiable: cuando Argos lo indica, el agente
  efectivamente está bloqueado esperando al usuario.
- El primer arranque muestra histórico inmediato, reconstruido de los logs ya existentes.

## 2. Contexto verificado

Los hallazgos siguientes fueron comprobados empíricamente en la máquina del usuario
(macOS, 2026-09-26) y son la base del diseño. Las rutas y formatos son privados y no
documentados; §9 y §10 tratan esa fragilidad.

### Fuentes por plataforma

| Vendor | Cliente CLI | Fuente en disco | cwd | Rama | Tokens | Subagentes |
|---|---|---|---|---|---|---|
| Anthropic | `claude` | `~/.claude/projects/<slug>/<uuid>.jsonl` | sí | sí (`gitBranch`) | sí (`usage`) | sí, ver abajo |
| OpenAI | `codex` | `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` | sí (`session_meta.cwd`) | derivable | por verificar | por verificar |
| Google | `gemini` | `~/.gemini/tmp/<proj>/chats/session-*.jsonl` | sí (`.project_root`) | derivable | por verificar | por verificar |
| Google | `agy` (Antigravity) | `~/.gemini/antigravity-cli/history.jsonl` | sí (`workspace`) | derivable | no | no |

Las celdas marcadas *por verificar* son desconocidos acotados, no requisitos vagos:
determinar si Codex y Gemini exponen conteo de tokens y subagentes es trabajo explícito
del probe correspondiente. El diseño no depende de la respuesta — las métricas son
opcionales por plataforma (§6) — así que cualquiera de los dos resultados es aceptable.

Notas relevantes:

- **Vendor y cliente son campos distintos.** Google expone dos clientes con formatos
  incompatibles entre sí (`gemini` usa JSONL por proyecto; `agy` un `history.jsonl`
  global con entradas `{display, timestamp, workspace}`). El usuario quiere ver la
  compañía, pero el parseo depende del cliente.
- **Claude Code registra subagentes en una jerarquía explícita:**
  `~/.claude/projects/<slug>/<sesión-uuid>/subagents/agent-<id>.jsonl`. La relación
  padre-hijo viene codificada en la ruta. Se observaron 372 invocaciones de la
  herramienta `Agent` en el historial del usuario: es un patrón constante, no un caso
  borde.
- `agy` es un binario Go en `~/.local/bin/agy`.

### Señales del sistema

- **cwd de un proceso:** `lsof -a -d cwd -p <pid>` funciona sin privilegios elevados.
- **Los CLIs no mantienen abierto su archivo de sesión.** Se verificó que un proceso
  `claude` vivo tiene cero descriptores `.jsonl` abiertos: escriben y cierran. Por lo
  tanto **no** es posible correlacionar proceso ↔ sesión vía descriptores de archivo.
- **Warp expone identidad de pane en el entorno del proceso**, legible con `ps -Ewwo`:

  ```
  WARP_TERMINAL_SESSION_UUID=15afe4b093924586a06cdcced8e499dd
  WARP_FOCUS_URL=warp://session/15afe4b093924586a06cdcced8e499dd
  ```

  `WARP_FOCUS_URL` enfoca esa pane exacta al abrirla. El objetivo 3 se resuelve con
  `open <WARP_FOCUS_URL>`, sin automatización de UI.
- **Árbol de procesos:** `claude → zsh → Warp` confirma la atribución del agente a su
  terminal.

## 3. Arquitectura

Workspace Cargo con dos crates:

- **`argos-core`** — librería, sin dependencias de UI. Contiene toda la observación,
  correlación, inferencia y persistencia.
- **`argos-gui`** — binario, interfaz `egui`/`eframe`. Consume `argos-core` y dibuja.
  No contiene lógica de negocio.

La separación existe para que un cambio de interfaz (a Tauri, a un dashboard web, a un
daemon con cliente) no toque la lógica. Es la única concesión a futuro que hace el
diseño; todo lo demás se construye para la necesidad presente.

### Componentes de `argos-core`

**Discovery** — determina el universo a vigilar: qué repositorios existen y, por cada
uno, sus worktrees y ramas (`git worktree list`, estado de cada rama). No sabe nada de
agentes.

**Probes** — recolectores tras un trait común. Cada uno lee *una* fuente y emite
observaciones crudas, sin interpretarlas:

- `ProcessProbe` — tabla de procesos, cwd vía `lsof`, entorno vía `ps -Ewwo` (de donde
  salen los identificadores de Warp).
- `ClaudeProbe`, `CodexProbe`, `GeminiProbe`, `AntigravityProbe` — uno por cliente CLI.

Añadir una plataforma nueva es añadir un probe, sin tocar el resto del sistema.

**Correlator** — cruza las observaciones de todos los probes para producir la jerarquía
completa. La llave de unión es la **ruta del filesystem**: el cwd del proceso, el campo
de ruta del archivo de sesión y la ruta del worktree se resuelven a la misma entidad.

**StateEngine** — infiere el estado de cada sesión a partir de las observaciones
correlacionadas. Es lógica pura: entra un conjunto de observaciones, sale un conjunto de
estados. Ver §5.

**Store** — SQLite. Guarda estado actual e histórico y sirve consultas al GUI.

El GUI hace polling al core en intervalos; el core no empuja hacia el GUI.

## 4. Correlación

La ruta une agente ↔ rama ↔ proyecto. El UUID de Warp identifica la pane y habilita el
salto.

**Ambigüedad conocida:** cuando varias sesiones del mismo cliente corren en el mismo
directorio (se observaron 4 sesiones concurrentes de Claude Code en un solo proyecto), no
hay señal exacta que empareje un proceso con su archivo de sesión, porque los CLIs no
mantienen el archivo abierto. El emparejamiento compara la hora de arranque del proceso
contra el primer timestamp de cada archivo de sesión candidato.

Esto es heurístico y el modelo lo trata como tal: toda correlación lleva un nivel de
confianza explícito. Una correlación dudosa se muestra como dudosa. El sistema nunca
presenta una inferencia incierta como si fuera un hecho.

## 5. Motor de estados

Cuatro estados:

| Estado | Condición |
|---|---|
| **Terminó** | Existe la sesión en disco, no hay proceso vivo asociado. |
| **Trabajando** | Proceso vivo y el agente está ejecutando. Ver regla de desempate. |
| **Esperando** | Proceso vivo, archivo de sesión quieto, última entrada es un mensaje del asistente que cerró turno. |
| **Desconocido** | Hay proceso sin sesión correlacionada, o sesión sin proceso identificable. |

### Regla de desempate

Distinguir *trabajando* de *esperando* por tiempo de inactividad del archivo **falla**: un
agente ejecutando un build de cinco minutos no escribe nada y se ve idéntico a uno
bloqueado esperando al usuario.

La señal decisiva es semántica, no temporal: **si la última entrada de la sesión contiene
un `tool_use` sin su `tool_result` correspondiente, el agente está trabajando**, por más
tiempo que lleve quieto el archivo. El tiempo de inactividad solo desempata cuando la
semántica es ambigua o la plataforma no expone detalle a nivel de herramienta.

**Desconocido** es un estado de primera clase, no un error. Es preferible que la
aplicación declare que no sabe a que afirme con confianza algo falso.

### Capacidades por plataforma

Cada probe declara qué puede aportar; la UI muestra lo conocido y marca lo que no lo es.
Antigravity, cuyo `history.jsonl` registra prompts con `workspace` y `timestamp` pero no
detalle de herramientas, obtiene correlación por ruta y actividad temporal, pero no la
distinción fina trabajando/esperando. Aparece en el tablero con su estado marcado como de
menor resolución en lugar de quedar fuera.

## 6. Modelo de datos

Jerarquía: `proyecto → rama/worktree → sesión de agente → subagente → actividad`.

**Un subagente es una sesión con padre.** No tiene tabla ni tipo propio: es la misma
entidad `AgentSession` con `parent_id` y `depth`. La recursión sale gratis — si un
subagente lanza otro, el modelo ya lo soporta.

`AgentSession` guarda: vendor, cliente CLI, id de sesión, ruta ancla, UUID de pane de
Warp, PID (si vive), inicio, última actividad, estado inferido, confianza de la
correlación, `parent_id`, `depth`.

Tabla append-only de **muestras** en el tiempo: cada ciclo de sondeo registra estado y
métricas de cada sesión viva. De ahí salen después las comparativas de rendimiento entre
plataformas. Las métricas son **opcionales por plataforma**: el esquema no exige que
todas expongan lo mismo.

## 7. Persistencia

**SQLite es un índice derivado, no la fuente de verdad.** Los archivos de sesión de los
agentes son la fuente: viven en disco independientemente de Argos y son reproducibles.

La base se puede borrar y reconstruir completa desde los logs con un comando de
reindexado. Tres consecuencias:

1. Las migraciones de esquema dejan de ser delicadas.
2. Un bug de ingesta se corrige reindexando, no arrastrando datos corruptos.
3. El primer arranque llena el histórico con los meses de sesiones ya acumuladas, sin
   haber monitoreado nada todavía.

## 8. Interfaz

Vista principal: árbol agrupado por proyecto → rama → agente → subagente.

**Orden por urgencia:** primero lo que espera respuesta del usuario, luego lo que trabaja,
al final lo terminado.

Cada fila: indicador de estado (color **y** forma, no solo color), vendor y cliente,
tiempo en el estado actual, tokens acumulados cuando la plataforma los expone.

Panel de detalle al seleccionar una fila: últimas acciones de la sesión, métricas, y
botón de salto a la pane de Warp (`open <WARP_FOCUS_URL>`).

Filtros por proyecto, plataforma y estado.

Fuera de v1: grafos de nodos, animaciones, timelines. `egui` cubre bien lo anterior; si
alguna de esas tres se vuelve necesaria, es el momento de evaluar Tauri, que cambiaría
solo `argos-gui`.

## 9. Manejo de errores

El riesgo estructural del proyecto es depender de formatos privados y no documentados que
pueden cambiar sin aviso en cualquier actualización de los CLIs.

- Cada probe está aislado: un parseo que falla degrada **esa** plataforma y no tumba la
  aplicación ni afecta a los demás probes.
- Parseo tolerante. Nunca `unwrap` sobre datos externos.
- Un probe degradado se refleja en la UI como tal; no desaparece en silencio.

## 10. Pruebas

El `StateEngine` es lógica pura sobre observaciones: se prueba sin procesos vivos ni
archivos reales. Es donde vive la complejidad y es completamente determinista.

Los probes se prueban contra archivos de muestra reales anonimizados, que cumplen doble
función: verifican el parseo y actúan como detector de cambios de formato. Si una
plataforma cambia su esquema, un test falla e indica exactamente cuál y por qué.

## 11. Riesgos

| Riesgo | Mitigación |
|---|---|
| Los formatos de los CLIs cambian sin aviso | Probes aislados, parseo tolerante, tests con muestras reales que detectan el cambio |
| Emparejamiento proceso ↔ sesión ambiguo con varias sesiones en el mismo directorio | Confianza explícita en el modelo; se muestra la duda en vez de ocultarla |
| Formato de Antigravity limitado | Degradación elegante: correlación por ruta sin estado de alta resolución |
| `egui` insuficiente para una visualización futura | `argos-core` aislado de la UI; migrar a Tauri tocaría solo `argos-gui` |

## 12. Decisiones descartadas

- **Daemon + cliente.** Su ventaja sería no perder histórico con la app cerrada, pero las
  fuentes de verdad son los logs que los CLIs escriben por su cuenta y persisten sin
  Argos. La app siempre puede reindexar hacia atrás, lo que elimina la razón principal
  para pagar la complejidad de dos binarios, launchd e IPC.
- **Tauri para v1.** Daría mejor visualización, pero introduce toolchain de frontend en un
  proyecto cuyo objetivo declarado incluye evaluar Rust.
- **Instrumentar los agentes** (que cada uno reporte su estado). Innecesario: los datos ya
  existen en disco.
- **Correlación proceso ↔ sesión por descriptor de archivo.** Verificado como inviable:
  los CLIs no mantienen los archivos abiertos.

## 13. Revisión: alcance por proyecto y rendimiento

Aprobada el 2026-09-26, después de ejecutar v1 contra datos reales. Supersede lo que
contradiga de las secciones anteriores.

### El problema medido

Un ciclo de sondeo tarda **1.65 s** y corre **en el hilo de la interfaz**, cada 3 s: la
ventana queda congelada aproximadamente la mitad del tiempo. Cada ciclo lee **728 MB en
445 archivos** —uno de ellos de 14 MB— y los vuelve a parsear enteros aunque no hayan
cambiado.

El diseño original asumía que vigilar la máquina entera era gratis. No lo es, y el costo
crece con el historial del usuario, así que optimizar sin cambiar el modelo solo aplaza
el problema.

### El cambio de modelo

**Argos deja de vigilar toda la máquina y vigila solo los proyectos que el usuario
elige.** Esto resuelve el rendimiento por diseño, no por optimización: el trabajo por
ciclo pasa a ser proporcional a lo que el usuario mira, no a lo que tiene en disco.

Cuatro piezas:

**Selector de proyectos como pantalla de entrada.** Lista los repositorios git
encontrados bajo las raíces de búsqueda, con selección múltiple. No parsea ni un solo
archivo de sesión: solo recorre directorios y consulta git, así que abre al instante y
muestra todos los proyectos, tengan o no agentes corriendo.

**El alcance viaja hasta los probes.** `SessionProbe::observe` recibe el conjunto de
proyectos a vigilar, y cada probe lo aplica **lo antes que su formato permita**, no
filtrando al final. Claude Code codifica la ruta en el nombre del directorio
(`-Users-alex-Proyectos-Orion`), así que descarta directorios enteros sin abrir un
archivo; Gemini se decide por `.project_root`; Codex necesita leer solo la primera línea
de cada sesión (`session_meta.cwd`) en vez del archivo completo; Antigravity lee su
historial global, que es pequeño. Este es el cambio de interfaz que hace arquitectónica
la revisión.

**El sondeo sale del hilo de la interfaz.** Corre en un hilo aparte y la UI lee el último
resultado disponible. Esto es lo que garantiza que la ventana responda pase lo que pase,
incluso con un proyecto de logs pesados: sin esto, cualquier proyecto grande volvería a
congelarla.

**Caché de parseo por archivo.** Con llave `(ruta, mtime, tamaño)`: un archivo que no
cambió no se vuelve a parsear. Dentro de un proyecto grande es la diferencia entre releer
cien megas o unos kilobytes.

### Persistencia de la selección

La selección del usuario se guarda y Argos arranca monitoreando esos proyectos, con una
vía de regreso al selector. Se guarda junto al resto del estado; como todo lo demás en
SQLite es derivado y reconstruible, la selección es el primer dato que **no** lo es, y por
eso vive en su propia tabla que el reindexado no toca.

### Consecuencia sobre la vista

Con varios proyectos seleccionados, la vista los agrupa por proyecto → rama → agente →
subagente, que es la jerarquía que §6 ya definía. El selector de proyecto de la iteración
anterior queda absorbido por esta pantalla de entrada.
