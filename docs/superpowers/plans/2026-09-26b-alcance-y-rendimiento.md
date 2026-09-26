# Alcance por proyecto y rendimiento — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Que Argos vigile solo los proyectos que el usuario elige, y que la ventana nunca se congele.

**Architecture:** El alcance elegido viaja desde la UI hasta cada probe, que lo aplica lo antes que su formato permite en vez de filtrar al final. El sondeo se muda a un hilo aparte y la UI lee el último resultado disponible. Un caché por `(ruta, mtime, tamaño)` evita reparsear lo que no cambió. La selección del usuario se persiste en una tabla que el reindexado no toca.

**Tech Stack:** Rust 1.91.1, `eframe`/`egui`, `rusqlite`, `serde_json`, `chrono`.

**Spec:** `docs/superpowers/specs/2026-09-26-argos-design.md` — en particular §13, que supersede lo que contradiga de las secciones anteriores.

## Global Constraints

Aplican las mismas de `AGENTS.md` y del plan anterior, más estas:

- **El alcance se aplica temprano, no al final.** Un probe que lee todo y luego filtra no cumple el objetivo aunque el resultado sea correcto. El trabajo por ciclo debe ser proporcional a lo elegido.
- **El prefiltro barato nunca debe producir falsos negativos.** Puede producir falsos positivos —que solo cuestan una lectura de más— y la confirmación exacta se hace con la ruta real que trae el archivo.
- **La UI nunca bloquea esperando al sondeo.** Ni al arrancar, ni al refrescar, ni al cambiar de proyecto.
- **La selección del usuario no es reconstruible.** Todo lo demás en SQLite es un índice derivado y `reset()` lo borra; la selección sobrevive al reindexado.
- **Sin dependencias nuevas** salvo que una tarea lo pida explícitamente. Los hilos van con `std::thread` y `std::sync`.

## Review Focus

Cinco clases de entrada que el spec implica y que ninguna tarea ejercitaría por defecto.

1. **Un proyecto hermano con nombre prefijo** (`/p/Orion` y `/p/Orion-old`): el prefiltro por slug de Claude Code daría un falso positivo. Debe resolverse con la ruta real del archivo, no quedarse con el prefijo. → Tarea 2.
2. **Selección vacía**: el usuario deselecciona todo o borra el proyecto que tenía guardado. Debe mostrar el selector y no sondear nada, en vez de caer en "vigilar todo" por defecto. → Tareas 6 y 8.
3. **Un archivo que cambia de tamaño sin cambiar mtime** (escrituras dentro del mismo segundo). El caché debe considerar el tamaño además de la fecha, o servirá datos viejos de una sesión activa. → Tarea 5.
4. **Un proyecto guardado que ya no existe en disco** cuando Argos arranca. Debe ignorarse sin romper el arranque ni borrar el resto de la selección. → Tarea 6.
5. **El hilo de sondeo entra en pánico.** La ventana debe seguir respondiendo y decirlo, no quedarse mostrando datos viejos en silencio para siempre. → Tarea 7.

## Orden

Secuencial: cada tarea consume la anterior. Las tareas 3 y 4 son independientes entre sí y podrían repartirse.

```
1 (Scope + trait) → 2 (Claude) → ┬→ 3 (Codex)
                                 └→ 4 (Gemini + Antigravity)
                                          ↓
                       5 (caché) → 6 (persistencia) → 7 (hilo) → 8 (UI)
```

---

### Task 1: El tipo `Scope` y el cambio de interfaz de los probes

**Files:**
- Create: `crates/argos-core/src/scope.rs`
- Modify: `crates/argos-core/src/lib.rs`
- Modify: `crates/argos-core/src/probes/mod.rs`
- Modify: `crates/argos-core/src/probes/{claude,codex,gemini,antigravity}.rs` (firma y filtrado provisional)
- Modify: `crates/argos-core/src/monitor.rs`

**Interfaces:**
- Consumes: `SessionObservation`, `ProbeError`.
- Produces: `Scope` con `Scope::all()`, `Scope::projects(Vec<PathBuf>)`, `Scope::contains(&Path) -> bool`, `Scope::is_empty() -> bool`, `Scope::roots() -> &[PathBuf]`; y la firma nueva `SessionProbe::observe(&self, scope: &Scope) -> Result<Vec<SessionObservation>, ProbeError>`.

En esta tarea los probes filtran **al final**, que es correcto pero no rápido. Las tareas 2 a 4 mueven el filtro hacia adelante en cada uno. Se hace así para que el árbol quede verde en cada paso.

- [ ] **Step 1: Escribir los tests que fallan**

Crear `crates/argos-core/src/scope.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn el_alcance_total_acepta_cualquier_ruta() {
        let s = Scope::all();
        assert!(s.contains(Path::new("/lo/que/sea")));
        assert!(!s.is_empty());
    }

    #[test]
    fn un_alcance_acotado_acepta_el_proyecto_y_lo_que_hay_dentro() {
        let s = Scope::projects(vec![PathBuf::from("/p/orion")]);

        assert!(s.contains(Path::new("/p/orion")), "el proyecto mismo");
        assert!(
            s.contains(Path::new("/p/orion/.worktrees/x")),
            "un worktree dentro"
        );
        assert!(!s.contains(Path::new("/p/otro")));
    }

    /// Review Focus #1: `/p/orion-old` no está dentro de `/p/orion`, por más
    /// que la cadena empiece igual.
    #[test]
    fn un_hermano_con_nombre_prefijo_no_cuenta_como_dentro() {
        let s = Scope::projects(vec![PathBuf::from("/p/orion")]);
        assert!(!s.contains(Path::new("/p/orion-old")));
        assert!(!s.contains(Path::new("/p/orion-old/src")));
    }

    /// Review Focus #2: sin selección no se vigila nada. "Vacío" no significa
    /// "todo": eso volvería a escanear la máquina entera por accidente.
    #[test]
    fn un_alcance_vacio_no_acepta_nada() {
        let s = Scope::projects(vec![]);
        assert!(s.is_empty());
        assert!(!s.contains(Path::new("/p/orion")));
    }
}
```

- [ ] **Step 2: Ejecutar los tests para verificar que fallan**

Añadir `pub mod scope;` a `crates/argos-core/src/lib.rs` antes de correr, o cargo no compila el módulo y reporta cero tests en vez de fallar.

Run: `cargo test -p argos-core scope`
Expected: FAIL, `Scope` no definido.

- [ ] **Step 3: Implementar `Scope`**

Al principio de `crates/argos-core/src/scope.rs`:

```rust
use std::path::{Path, PathBuf};

/// Qué proyectos vigila Argos en este ciclo.
///
/// `Projects(vec![])` significa **nada**, no todo: un alcance vacío por
/// descuido volvería a escanear la máquina entera, que es justo lo que esta
/// revisión elimina.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    All,
    Projects(Vec<PathBuf>),
}

impl Scope {
    pub fn all() -> Self {
        Scope::All
    }

    pub fn projects(roots: Vec<PathBuf>) -> Self {
        Scope::Projects(roots)
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Scope::Projects(r) if r.is_empty())
    }

    pub fn roots(&self) -> &[PathBuf] {
        match self {
            Scope::All => &[],
            Scope::Projects(r) => r,
        }
    }

    /// `starts_with` de `Path` compara por segmentos, no por cadena, así que
    /// `/p/orion-old` no cae dentro de `/p/orion`.
    pub fn contains(&self, path: &Path) -> bool {
        match self {
            Scope::All => true,
            Scope::Projects(roots) => roots.iter().any(|r| path.starts_with(r)),
        }
    }
}
```

- [ ] **Step 4: Ejecutar los tests para verificar que pasan**

Run: `cargo test -p argos-core scope`
Expected: PASS, 4 tests.

- [ ] **Step 5: Cambiar la firma del trait y los cuatro probes**

En `crates/argos-core/src/probes/mod.rs`:

```rust
use crate::scope::Scope;

pub trait SessionProbe: Send + Sync {
    fn client(&self) -> ClientKind;

    fn capabilities(&self) -> Capabilities;

    /// `scope` acota qué proyectos interesan. Cada implementación debe
    /// aplicarlo **lo antes que su formato permita**, no al final.
    fn observe(&self, scope: &Scope) -> Result<Vec<SessionObservation>, ProbeError>;
}
```

En cada uno de `claude.rs`, `codex.rs`, `gemini.rs`, `antigravity.rs`, cambiar la firma y filtrar al final por ahora. El patrón es el mismo en los cuatro; ejemplo para `claude.rs`:

```rust
    fn observe(&self, scope: &Scope) -> Result<Vec<SessionObservation>, ProbeError> {
        if scope.is_empty() {
            return Ok(Vec::new());
        }
        // ... el cuerpo actual, sin cambios ...
        sessions.retain(|s| scope.contains(&s.anchor_path));
        Ok(sessions)
    }
```

Añadir `use crate::scope::Scope;` en cada archivo.

- [ ] **Step 6: Propagar el alcance por el monitor**

En `crates/argos-core/src/monitor.rs`: añadir `pub scope: Scope` a `MonitorConfig` (por defecto `Scope::projects(vec![])`, o sea nada hasta que el usuario elija), pasar `&self.config.scope` a cada `probe.observe(...)` dentro de `collect`, y añadir un parámetro `scope: &Scope` a `collect`. Actualizar las llamadas de los tests existentes con `&Scope::all()`.

En `MonitorConfig::default()`, dejar `scope: Scope::all()` **solo** para que `argos-probe` siga sirviendo de diagnóstico completo; la GUI construirá el suyo acotado.

- [ ] **Step 7: Escribir el test de que el alcance llega a los probes**

Añadir al bloque de tests de `monitor.rs`:

```rust
    /// El alcance tiene que llegar hasta el probe, no filtrarse después.
    #[test]
    fn el_alcance_llega_hasta_el_probe() {
        use crate::scope::Scope;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        struct Espia(Arc<AtomicUsize>);

        impl SessionProbe for Espia {
            fn client(&self) -> ClientKind {
                ClientKind::ClaudeCode
            }
            fn capabilities(&self) -> Capabilities {
                Capabilities::full()
            }
            fn observe(&self, scope: &Scope) -> Result<Vec<SessionObservation>, ProbeError> {
                self.0.store(scope.roots().len(), Ordering::Relaxed);
                Ok(Vec::new())
            }
        }

        let visto = Arc::new(AtomicUsize::new(0));
        let probes: Vec<Box<dyn SessionProbe>> = vec![Box::new(Espia(visto.clone()))];
        let scope = Scope::projects(vec![PathBuf::from("/p/a"), PathBuf::from("/p/b")]);

        collect(&probes, &[], &[], Utc::now(), DEFAULT_IDLE_THRESHOLD, &scope);

        assert_eq!(visto.load(Ordering::Relaxed), 2, "el probe debe ver el alcance");
    }
```

- [ ] **Step 8: Verificar y comprobar que no hay regresión**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS, sin advertencias. El conteo total sube en 5 respecto al actual.

- [ ] **Step 9: Commit**

```bash
git add crates
git commit -m "feat: el alcance de monitoreo viaja hasta los probes"
```

---

### Task 2: ClaudeProbe descarta directorios sin abrir archivos

**Files:**
- Modify: `crates/argos-core/src/probes/claude.rs`

**Interfaces:**
- Consumes: `Scope`.
- Produces: `slug_de_proyecto(&Path) -> String` y `slug_en_alcance(nombre_dir: &str, scope: &Scope) -> bool`, ambas públicas y puras.

Esta es la tarea con el mayor efecto: Claude Code codifica la ruta del proyecto en el nombre del directorio (`-Users-alex-Proyectos-Orion`), así que se pueden descartar directorios enteros —cientos de megas— sin abrir un solo archivo.

**El prefiltro es aproximado a propósito.** Codificar es determinista, pero decodificar no: un guion en el nombre real de una carpeta es indistinguible del separador. Por eso el prefiltro solo garantiza **no descartar de más**, y la confirmación exacta se hace después con el campo `cwd` que trae el archivo, que ya se parsea hoy.

- [ ] **Step 1: Escribir los tests que fallan**

Añadir al bloque de tests de `claude.rs`:

```rust
    use crate::scope::Scope;

    #[test]
    fn la_ruta_del_proyecto_se_codifica_como_el_nombre_del_directorio() {
        assert_eq!(
            slug_de_proyecto(Path::new("/Users/alex/Proyectos/Orion")),
            "-Users-alex-Proyectos-Orion"
        );
    }

    #[test]
    fn el_directorio_del_proyecto_y_los_de_sus_worktrees_estan_en_alcance() {
        let scope = Scope::projects(vec![PathBuf::from("/Users/alex/Proyectos/Orion")]);

        assert!(slug_en_alcance("-Users-alex-Proyectos-Orion", &scope));
        assert!(slug_en_alcance(
            "-Users-alex-Proyectos-Orion--worktrees-adam-slm",
            &scope
        ));
    }

    #[test]
    fn un_directorio_de_otro_proyecto_se_descarta() {
        let scope = Scope::projects(vec![PathBuf::from("/Users/alex/Proyectos/Orion")]);
        assert!(!slug_en_alcance("-Users-alex-Proyectos-Laboratorio", &scope));
    }

    /// Review Focus #1: el prefiltro deja pasar al hermano con nombre prefijo
    /// —no puede distinguirlo sin leer— y la confirmación exacta la hace el
    /// `cwd` del archivo. Lo que NO puede hacer es descartarlo de menos.
    #[test]
    fn el_prefiltro_prefiere_un_falso_positivo_antes_que_perder_una_sesion() {
        let scope = Scope::projects(vec![PathBuf::from("/Users/alex/Proyectos/Orion")]);

        assert!(
            slug_en_alcance("-Users-alex-Proyectos-Orion-old", &scope),
            "no puede distinguirlo sin leer: pasa y se confirma con el cwd"
        );

        // Y la confirmación exacta sí lo descarta.
        assert!(!scope.contains(Path::new("/Users/alex/Proyectos/Orion-old")));
    }

    #[test]
    fn con_alcance_total_todo_directorio_pasa() {
        assert!(slug_en_alcance("-lo-que-sea", &Scope::all()));
    }
```

- [ ] **Step 2: Ejecutar los tests para verificar que fallan**

Run: `cargo test -p argos-core claude`
Expected: FAIL, `slug_de_proyecto` y `slug_en_alcance` no definidas.

- [ ] **Step 3: Implementar el prefiltro**

Añadir a `crates/argos-core/src/probes/claude.rs`:

```rust
/// Claude Code nombra el directorio de un proyecto sustituyendo `/` por `-`.
/// Codificar es determinista; decodificar no lo es, porque un guion del nombre
/// real es indistinguible del separador.
pub fn slug_de_proyecto(project: &Path) -> String {
    project.to_string_lossy().replace('/', "-")
}

/// Prefiltro barato: decide si vale la pena mirar dentro de un directorio.
/// Puede dejar pasar de más (se confirma luego con el `cwd` del archivo), pero
/// nunca debe descartar de menos.
pub fn slug_en_alcance(nombre_dir: &str, scope: &Scope) -> bool {
    match scope {
        Scope::All => true,
        Scope::Projects(roots) => roots.iter().any(|r| {
            let slug = slug_de_proyecto(r);
            nombre_dir == slug || nombre_dir.starts_with(&format!("{slug}-"))
        }),
    }
}
```

- [ ] **Step 4: Ejecutar los tests para verificar que pasan**

Run: `cargo test -p argos-core claude`
Expected: PASS, los 5 nuevos incluidos.

- [ ] **Step 5: Usar el prefiltro en el recorrido**

Reemplazar el cuerpo de `observe` en `impl SessionProbe for ClaudeProbe`:

```rust
    fn observe(&self, scope: &Scope) -> Result<Vec<SessionObservation>, ProbeError> {
        if scope.is_empty() {
            return Ok(Vec::new());
        }
        if !self.root.exists() {
            return Err(ProbeError::SourceMissing(self.root.clone()));
        }

        let mut sessions = Vec::new();

        let Ok(proyectos) = std::fs::read_dir(&self.root) else {
            return Ok(sessions);
        };

        for entrada in proyectos.filter_map(Result::ok) {
            let dir = entrada.path();
            let Some(nombre) = dir.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            // Aquí se descartan cientos de megas sin abrir un archivo.
            if !slug_en_alcance(nombre, scope) {
                continue;
            }

            for entry in walkdir::WalkDir::new(&dir).into_iter().filter_map(Result::ok) {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                let Some(identity) = identity_from_path(path) else {
                    continue;
                };
                let Ok(contents) = std::fs::read_to_string(path) else {
                    continue;
                };
                if let Some(observation) =
                    parse_session(&contents, path, identity.id, identity.parent_id)
                {
                    // Confirmación exacta: el prefiltro pudo dejar pasar un
                    // hermano con nombre prefijo.
                    if scope.contains(&observation.anchor_path) {
                        sessions.push(observation);
                    }
                }
            }
        }

        Ok(sessions)
    }
```

- [ ] **Step 6: Medir la mejora contra los datos reales**

Run:

```bash
cargo build --release -p argos-core --bin argos-probe
time ./target/release/argos-probe > /dev/null
```

Expected: sigue tardando ~1.6 s, porque `argos-probe` usa `Scope::all()`. Eso es correcto: confirma que no se rompió el diagnóstico completo. La mejora se mide en la Tarea 8, con la GUI acotada.

- [ ] **Step 7: Commit**

```bash
git add crates
git commit -m "perf: Claude Code descarta directorios fuera de alcance sin leerlos"
```

---

### Task 3: CodexProbe lee solo la cabecera para decidir

**Files:**
- Modify: `crates/argos-core/src/probes/codex.rs`

**Interfaces:**
- Consumes: `Scope`.
- Produces: `cwd_de_cabecera(primera_linea: &str) -> Option<PathBuf>`.

Codex no codifica la ruta en el nombre del archivo, pero sí la pone en la **primera línea** (`session_meta.payload.cwd`). Leer una línea en vez del archivo completo es la diferencia.

- [ ] **Step 1: Escribir los tests que fallan**

Añadir al bloque de tests de `codex.rs`:

```rust
    #[test]
    fn la_cabecera_basta_para_saber_el_cwd() {
        let primera = r#"{"timestamp":"2026-08-31T20:22:58.163Z","type":"session_meta","payload":{"session_id":"x","cwd":"/p/orion/.worktrees/a"}}"#;
        assert_eq!(
            cwd_de_cabecera(primera),
            Some(PathBuf::from("/p/orion/.worktrees/a"))
        );
    }

    #[test]
    fn una_cabecera_que_no_es_session_meta_no_da_cwd() {
        assert_eq!(
            cwd_de_cabecera(r#"{"type":"event_msg","payload":{}}"#),
            None
        );
        assert_eq!(cwd_de_cabecera("basura no json"), None);
        assert_eq!(cwd_de_cabecera(""), None);
    }
```

- [ ] **Step 2: Ejecutar los tests para verificar que fallan**

Run: `cargo test -p argos-core codex`
Expected: FAIL, `cwd_de_cabecera` no definida.

- [ ] **Step 3: Implementar**

Añadir a `crates/argos-core/src/probes/codex.rs`:

```rust
use std::io::{BufRead, BufReader};

/// El `cwd` vive en la primera línea del rollout, así que se puede decidir el
/// alcance sin leer el resto del archivo.
pub fn cwd_de_cabecera(primera_linea: &str) -> Option<PathBuf> {
    let entry: Value = serde_json::from_str(primera_linea).ok()?;
    if entry.get("type").and_then(Value::as_str) != Some("session_meta") {
        return None;
    }
    entry
        .get("payload")?
        .get("cwd")
        .and_then(Value::as_str)
        .map(PathBuf::from)
}

fn primera_linea(path: &Path) -> Option<String> {
    let archivo = std::fs::File::open(path).ok()?;
    BufReader::new(archivo).lines().next()?.ok()
}
```

Y en `observe`, antes de leer el archivo entero:

```rust
            if !matches!(scope, Scope::All) {
                let dentro = primera_linea(&path)
                    .and_then(|l| cwd_de_cabecera(&l))
                    .map(|cwd| scope.contains(&cwd))
                    .unwrap_or(false);
                if !dentro {
                    continue;
                }
            }
```

- [ ] **Step 4: Ejecutar los tests para verificar que pasan**

Run: `cargo test -p argos-core codex`
Expected: PASS, los 2 nuevos incluidos.

- [ ] **Step 5: Commit**

```bash
git add crates
git commit -m "perf: Codex decide el alcance leyendo solo la cabecera"
```

---

### Task 4: Gemini y Antigravity aplican el alcance temprano

**Files:**
- Modify: `crates/argos-core/src/probes/gemini.rs`
- Modify: `crates/argos-core/src/probes/antigravity.rs`

**Interfaces:**
- Consumes: `Scope`, `read_project_root`, `parse_history`.
- Produces: `parse_history(contents, source, scope)` con el alcance aplicado durante el agrupado.

Gemini es fácil: `.project_root` dice a qué proyecto pertenece un directorio antes de abrir ningún chat. Antigravity lee un archivo global pequeño (unos cientos de KB), así que el alcance se aplica durante el agrupado.

- [ ] **Step 1: Escribir los tests que fallan**

En `gemini.rs`:

```rust
    #[test]
    fn un_proyecto_fuera_de_alcance_no_abre_sus_chats() {
        use crate::scope::Scope;

        let dir = tempdir("fuera-de-alcance");
        let proyecto = dir.join("orion");
        std::fs::create_dir_all(proyecto.join("chats")).expect("crear dirs");
        std::fs::write(proyecto.join(".project_root"), "/p/orion\n").expect("escribir");
        std::fs::write(
            proyecto.join("chats/session-x.jsonl"),
            "{\"role\":\"user\"}\n",
        )
        .expect("escribir chat");

        let probe = GeminiProbe::new(dir.clone());

        let dentro = probe
            .observe(&Scope::projects(vec![PathBuf::from("/p/orion")]))
            .expect("observar");
        assert_eq!(dentro.len(), 1);

        let fuera = probe
            .observe(&Scope::projects(vec![PathBuf::from("/p/otro")]))
            .expect("observar");
        assert!(fuera.is_empty(), "no debe devolver chats de otro proyecto");
    }
```

En `antigravity.rs`:

```rust
    #[test]
    fn el_historial_global_se_filtra_por_alcance() {
        use crate::scope::Scope;

        let sesiones = parse_history(
            &leer(),
            Path::new("/x/history.jsonl"),
            &Scope::projects(vec![PathBuf::from("/Users/alex/Proyectos/Orion")]),
        );

        assert_eq!(sesiones.len(), 1, "solo el workspace en alcance");
        assert_eq!(
            sesiones[0].anchor_path,
            PathBuf::from("/Users/alex/Proyectos/Orion")
        );
    }
```

- [ ] **Step 2: Ejecutar los tests para verificar que fallan**

Run: `cargo test -p argos-core gemini antigravity`
Expected: FAIL — `parse_history` no acepta tres argumentos; el de Gemini falla la aserción de `fuera`.

- [ ] **Step 3: Implementar**

En `gemini.rs`, dentro de `observe`, justo después de resolver el ancla:

```rust
            let Some(anchor) = read_project_root(&project_dir) else {
                continue;
            };
            // Se decide antes de abrir el directorio de chats.
            if !scope.contains(&anchor) {
                continue;
            }
```

En `antigravity.rs`, añadir el parámetro y filtrar al agrupar:

```rust
pub fn parse_history(contents: &str, source: &Path, scope: &Scope) -> Vec<SessionObservation> {
    // ... dentro del bucle, tras leer `workspace`:
        if !scope.contains(Path::new(workspace)) {
            continue;
        }
```

Actualizar la llamada en `observe` y los tests existentes de `parse_history` para pasar `&Scope::all()`.

- [ ] **Step 4: Ejecutar los tests para verificar que pasan**

Run: `cargo test -p argos-core`
Expected: PASS, todo verde.

- [ ] **Step 5: Commit**

```bash
git add crates
git commit -m "perf: Gemini y Antigravity aplican el alcance antes de leer"
```

---

### Task 5: Caché de parseo por archivo

**Files:**
- Create: `crates/argos-core/src/cache.rs`
- Modify: `crates/argos-core/src/lib.rs`
- Modify: `crates/argos-core/src/probes/claude.rs`

**Interfaces:**
- Consumes: `SessionObservation`.
- Produces: `ParseCache::new()`, `ParseCache::get_or_parse(&self, path: &Path, f: impl FnOnce(&str) -> Option<SessionObservation>) -> Option<SessionObservation>`, y `Huella::de(path) -> Option<Huella>`.

Cubre el punto 3 de Review Focus: mtime con granularidad de segundo.

El caché usa interior mutability (`Mutex`) porque `SessionProbe` exige `Send + Sync` y `observe` toma `&self`.

- [ ] **Step 1: Escribir los tests que fallan**

Crear `crates/argos-core/src/cache.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn ruta_temporal(nombre: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "argos-cache-{}-{nombre}.jsonl",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn un_archivo_que_no_cambio_no_se_vuelve_a_parsear() {
        let ruta = ruta_temporal("estable");
        std::fs::write(&ruta, "contenido").expect("escribir");

        let cache = ParseCache::new();
        let veces = AtomicUsize::new(0);

        let parsear = |_: &str| {
            veces.fetch_add(1, Ordering::Relaxed);
            None::<crate::observation::SessionObservation>
        };

        cache.get_or_parse(&ruta, parsear);
        cache.get_or_parse(&ruta, parsear);

        assert_eq!(veces.load(Ordering::Relaxed), 1, "solo la primera vez");
        let _ = std::fs::remove_file(&ruta);
    }

    /// Review Focus #3: dos escrituras dentro del mismo segundo pueden dejar
    /// el mtime igual. Sin el tamaño en la huella, el caché serviría datos
    /// viejos de una sesión que está activa justo ahora.
    #[test]
    fn un_cambio_de_tamano_invalida_aunque_el_mtime_no_se_mueva() {
        let ruta = ruta_temporal("mismo-segundo");
        std::fs::write(&ruta, "corto").expect("escribir");
        let antes = Huella::de(&ruta).expect("huella");

        std::fs::write(&ruta, "mucho mas largo que antes").expect("reescribir");
        let mut despues = Huella::de(&ruta).expect("huella");
        despues.mtime = antes.mtime; // simula el mismo segundo

        assert_ne!(antes, despues, "el tamaño tiene que distinguirlas");
        let _ = std::fs::remove_file(&ruta);
    }

    #[test]
    fn un_archivo_que_desaparece_no_entra_en_panico() {
        assert!(Huella::de(Path::new("/no/existe/jamas")).is_none());
    }
}
```

- [ ] **Step 2: Ejecutar los tests para verificar que fallan**

Añadir `pub mod cache;` a `lib.rs`.

Run: `cargo test -p argos-core cache`
Expected: FAIL, `ParseCache` y `Huella` no definidos.

- [ ] **Step 3: Implementar**

Al principio de `crates/argos-core/src/cache.rs`:

```rust
use crate::observation::SessionObservation;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// Identidad barata de un archivo. El tamaño acompaña al mtime porque en
/// macOS dos escrituras dentro del mismo segundo pueden dejar la fecha igual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Huella {
    pub mtime: SystemTime,
    pub tamano: u64,
}

impl Huella {
    pub fn de(path: &Path) -> Option<Huella> {
        let md = std::fs::metadata(path).ok()?;
        Some(Huella {
            mtime: md.modified().ok()?,
            tamano: md.len(),
        })
    }
}

/// Evita reparsear archivos que no cambiaron entre ciclos de sondeo.
#[derive(Default)]
pub struct ParseCache {
    entradas: Mutex<HashMap<PathBuf, (Huella, Option<SessionObservation>)>>,
}

impl ParseCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_or_parse<F>(&self, path: &Path, parsear: F) -> Option<SessionObservation>
    where
        F: FnOnce(&str) -> Option<SessionObservation>,
    {
        let huella = Huella::de(path)?;

        if let Ok(entradas) = self.entradas.lock()
            && let Some((previa, resultado)) = entradas.get(path)
            && previa == &huella
        {
            return resultado.clone();
        }

        let contenido = std::fs::read_to_string(path).ok()?;
        let resultado = parsear(&contenido);

        if let Ok(mut entradas) = self.entradas.lock() {
            entradas.insert(path.to_path_buf(), (huella, resultado.clone()));
        }

        resultado
    }
}
```

- [ ] **Step 4: Ejecutar los tests para verificar que pasan**

Run: `cargo test -p argos-core cache`
Expected: PASS, 3 tests.

- [ ] **Step 5: Usarlo en el probe de Claude Code**

Añadir el campo `cache: ParseCache` a `ClaudeProbe`, inicializarlo en `new`, y en `observe` sustituir el par leer-parsear:

```rust
                let SessionIdentity { id, parent_id } = identity;
                let Some(observation) = self
                    .cache
                    .get_or_parse(path, move |contents| {
                        parse_session(contents, path, id, parent_id)
                    })
                else {
                    continue;
                };
```

- [ ] **Step 6: Verificar que sigue todo verde**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS, sin advertencias.

- [ ] **Step 7: Commit**

```bash
git add crates
git commit -m "perf: caché de parseo por mtime y tamaño"
```

---

### Task 6: Persistir la selección de proyectos

**Files:**
- Modify: `crates/argos-core/src/store.rs`

**Interfaces:**
- Consumes: `Store`.
- Produces: `Store::save_watched(&[PathBuf]) -> Result<(), StoreError>` y `Store::watched() -> Result<Vec<PathBuf>, StoreError>`.

Cubre los puntos 2 y 4 de Review Focus.

La selección es el primer dato **no** reconstruible del sistema, así que vive en su propia tabla y `reset()` no la toca. Subir `SCHEMA_VERSION` a 3.

- [ ] **Step 1: Escribir los tests que fallan**

Añadir al bloque de tests de `store.rs`:

```rust
    #[test]
    fn guarda_y_recupera_los_proyectos_vigilados() {
        let store = Store::in_memory().expect("abrir");
        let elegidos = vec![PathBuf::from("/p/orion"), PathBuf::from("/p/lab")];

        store.save_watched(&elegidos).expect("guardar");
        assert_eq!(store.watched().expect("leer"), elegidos);
    }

    #[test]
    fn guardar_reemplaza_la_seleccion_anterior_en_vez_de_acumular() {
        let store = Store::in_memory().expect("abrir");
        store.save_watched(&[PathBuf::from("/p/a")]).expect("1");
        store.save_watched(&[PathBuf::from("/p/b")]).expect("2");

        assert_eq!(store.watched().expect("leer"), vec![PathBuf::from("/p/b")]);
    }

    /// Review Focus #2: deseleccionar todo es una elección válida y debe
    /// persistir como tal, no revertir a la selección anterior.
    #[test]
    fn una_seleccion_vacia_se_guarda_como_vacia() {
        let store = Store::in_memory().expect("abrir");
        store.save_watched(&[PathBuf::from("/p/a")]).expect("1");
        store.save_watched(&[]).expect("vaciar");

        assert!(store.watched().expect("leer").is_empty());
    }

    /// La selección es el único dato no reconstruible: el reindexado, que
    /// tira y rehace todo lo derivado, no debe llevársela por delante.
    #[test]
    fn el_reindexado_no_borra_la_seleccion() {
        let store = Store::in_memory().expect("abrir");
        store.save_watched(&[PathBuf::from("/p/orion")]).expect("guardar");
        store
            .upsert_snapshot(&[fila("s1", AgentState::Working)])
            .expect("sesión");

        store.reset().expect("reset");

        assert!(store.current().expect("leer").is_empty(), "lo derivado sí se va");
        assert_eq!(
            store.watched().expect("leer"),
            vec![PathBuf::from("/p/orion")],
            "la selección sobrevive"
        );
    }
```

- [ ] **Step 2: Ejecutar los tests para verificar que fallan**

Run: `cargo test -p argos-core store`
Expected: FAIL, `save_watched` y `watched` no definidos.

- [ ] **Step 3: Implementar**

Subir la versión y añadir la tabla en `store.rs`:

```rust
const SCHEMA_VERSION: i64 = 3;
```

`DROP` sigue tirando solo `sessions` y `samples`: la tabla de selección no se toca al cambiar de versión, porque su contenido no se puede regenerar desde los logs.

Añadir al final de `SCHEMA`:

```sql
CREATE TABLE IF NOT EXISTS watched_projects (
    posicion INTEGER PRIMARY KEY,
    path     TEXT NOT NULL
);
```

Y los métodos:

```rust
    /// Reemplaza la selección completa. Guardar una lista vacía es una
    /// elección legítima del usuario y se persiste como tal.
    pub fn save_watched(&self, projects: &[PathBuf]) -> Result<(), StoreError> {
        self.conn.execute("DELETE FROM watched_projects", [])?;
        for (i, p) in projects.iter().enumerate() {
            self.conn.execute(
                "INSERT INTO watched_projects (posicion, path) VALUES (?1, ?2)",
                params![i as i64, p.to_string_lossy()],
            )?;
        }
        Ok(())
    }

    pub fn watched(&self) -> Result<Vec<PathBuf>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT path FROM watched_projects ORDER BY posicion")?;
        let filas = stmt.query_map([], |r| Ok(PathBuf::from(r.get::<_, String>(0)?)))?;
        filas.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
    }
```

- [ ] **Step 4: Ejecutar los tests para verificar que pasan**

Run: `cargo test -p argos-core store`
Expected: PASS, los 4 nuevos incluidos.

- [ ] **Step 5: Commit**

```bash
git add crates
git commit -m "feat: persiste la selección de proyectos fuera del índice derivado"
```

---

### Task 7: El sondeo se muda a un hilo aparte

**Files:**
- Create: `crates/argos-core/src/watcher.rs`
- Modify: `crates/argos-core/src/lib.rs`

**Interfaces:**
- Consumes: `Monitor`, `MonitorConfig`, `Snapshot`, `Scope`.
- Produces: `Watcher::start(config: MonitorConfig, intervalo: Duration) -> Watcher`, `Watcher::latest() -> Option<Snapshot>`, `Watcher::set_scope(Scope)`, `Watcher::estado() -> EstadoSondeo`, `Watcher::stop()`.

Cubre el punto 5 de Review Focus.

Esta es la pieza que hace que la ventana responda pase lo que pase. El hilo sondea y publica; la UI solo lee lo último publicado y nunca espera.

- [ ] **Step 1: Escribir los tests que fallan**

Crear `crates/argos-core/src/watcher.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn publica_un_resultado_sin_que_el_llamador_espere() {
        let config = MonitorConfig {
            scope: Scope::projects(vec![]),
            ..Default::default()
        };
        let w = Watcher::start(config, Duration::from_millis(20));

        // `latest()` no bloquea: al principio puede no haber nada todavía.
        let _ = w.latest();

        let limite = std::time::Instant::now() + Duration::from_secs(5);
        while w.latest().is_none() && std::time::Instant::now() < limite {
            std::thread::sleep(Duration::from_millis(10));
        }

        assert!(w.latest().is_some(), "el hilo debe publicar un snapshot");
        w.stop();
    }

    #[test]
    fn cambiar_el_alcance_se_refleja_en_el_siguiente_ciclo() {
        let config = MonitorConfig {
            scope: Scope::projects(vec![]),
            ..Default::default()
        };
        let w = Watcher::start(config, Duration::from_millis(20));
        w.set_scope(Scope::projects(vec![PathBuf::from("/p/nuevo")]));

        let limite = std::time::Instant::now() + Duration::from_secs(5);
        while w.scope_actual() != Scope::projects(vec![PathBuf::from("/p/nuevo")])
            && std::time::Instant::now() < limite
        {
            std::thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(
            w.scope_actual(),
            Scope::projects(vec![PathBuf::from("/p/nuevo")])
        );
        w.stop();
    }

    /// Review Focus #5: si el hilo muere, la ventana no puede quedarse
    /// mostrando datos viejos en silencio para siempre.
    #[test]
    fn si_el_hilo_muere_el_estado_lo_dice() {
        let w = Watcher::start(
            MonitorConfig {
                scope: Scope::projects(vec![]),
                ..Default::default()
            },
            Duration::from_millis(20),
        );
        assert_eq!(w.estado(), EstadoSondeo::Vivo);

        w.stop();
        let limite = std::time::Instant::now() + Duration::from_secs(5);
        while w.estado() == EstadoSondeo::Vivo && std::time::Instant::now() < limite {
            std::thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(w.estado(), EstadoSondeo::Detenido);
    }
}
```

- [ ] **Step 2: Ejecutar los tests para verificar que fallan**

Añadir `pub mod watcher;` a `lib.rs`.

Run: `cargo test -p argos-core watcher`
Expected: FAIL, `Watcher` no definido.

- [ ] **Step 3: Implementar**

Al principio de `crates/argos-core/src/watcher.rs`:

```rust
use crate::monitor::{Monitor, MonitorConfig, Snapshot};
use crate::scope::Scope;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoSondeo {
    Vivo,
    Detenido,
}

struct Compartido {
    ultimo: Mutex<Option<Snapshot>>,
    scope: Mutex<Scope>,
    corriendo: AtomicBool,
    vivo: AtomicBool,
}

/// Sondea en un hilo aparte y publica el último resultado. La UI lee sin
/// esperar nunca: un ciclo lento retrasa la actualización, no la ventana.
pub struct Watcher {
    compartido: Arc<Compartido>,
    hilo: Option<JoinHandle<()>>,
}

impl Watcher {
    pub fn start(config: MonitorConfig, intervalo: Duration) -> Watcher {
        let compartido = Arc::new(Compartido {
            ultimo: Mutex::new(None),
            scope: Mutex::new(config.scope.clone()),
            corriendo: AtomicBool::new(true),
            vivo: AtomicBool::new(true),
        });

        let c = compartido.clone();
        let hilo = std::thread::spawn(move || {
            let monitor = Monitor::new(config);

            while c.corriendo.load(Ordering::Relaxed) {
                let scope = c.scope.lock().map(|s| s.clone()).unwrap_or(Scope::All);
                let snapshot = monitor.poll_con_alcance(&scope);

                if let Ok(mut ultimo) = c.ultimo.lock() {
                    *ultimo = Some(snapshot);
                }

                std::thread::sleep(intervalo);
            }

            c.vivo.store(false, Ordering::Relaxed);
        });

        Watcher {
            compartido,
            hilo: Some(hilo),
        }
    }

    /// No bloquea. Devuelve `None` hasta que hay un primer resultado.
    pub fn latest(&self) -> Option<Snapshot> {
        self.compartido.ultimo.lock().ok()?.clone()
    }

    pub fn set_scope(&self, scope: Scope) {
        if let Ok(mut s) = self.compartido.scope.lock() {
            *s = scope;
        }
    }

    pub fn scope_actual(&self) -> Scope {
        self.compartido
            .scope
            .lock()
            .map(|s| s.clone())
            .unwrap_or(Scope::All)
    }

    pub fn estado(&self) -> EstadoSondeo {
        if self.compartido.vivo.load(Ordering::Relaxed) {
            EstadoSondeo::Vivo
        } else {
            EstadoSondeo::Detenido
        }
    }

    pub fn stop(&self) {
        self.compartido.corriendo.store(false, Ordering::Relaxed);
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop();
        if let Some(h) = self.hilo.take() {
            let _ = h.join();
        }
    }
}
```

En `monitor.rs`, añadir el método que toma el alcance por parámetro y hacer que `poll` delegue:

```rust
    pub fn poll(&self) -> Snapshot {
        self.poll_con_alcance(&self.config.scope.clone())
    }

    pub fn poll_con_alcance(&self, scope: &Scope) -> Snapshot {
        // el cuerpo actual de poll, pasando `scope` a `collect`
    }
```

`Scope` necesita derivar `Clone` (lo hace) porque el hilo lee una copia en cada ciclo.

- [ ] **Step 4: Ejecutar los tests para verificar que pasan**

Run: `cargo test -p argos-core watcher`
Expected: PASS, 3 tests.

- [ ] **Step 5: Commit**

```bash
git add crates
git commit -m "perf: el sondeo corre en un hilo aparte y la UI nunca espera"
```

---

### Task 8: Selector de proyectos y vista acotada

**Files:**
- Modify: `crates/argos-gui/src/app.rs`
- Create: `crates/argos-gui/src/selector.rs`
- Modify: `crates/argos-gui/src/main.rs`

**Interfaces:**
- Consumes: `Watcher`, `Scope`, `Store`, `find_repos`, `summarize_projects`, `group_by_branch`.
- Produces: `ProyectoDisponible { path, nombre, seleccionado }` y `proyectos_disponibles(roots: &[PathBuf], max_depth: usize, seleccion: &[PathBuf]) -> Vec<ProyectoDisponible>`.

Cubre el punto 2 de Review Focus.

- [ ] **Step 1: Escribir los tests que fallan**

Crear `crates/argos-gui/src/selector.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn marca_como_seleccionados_los_que_vienen_de_la_seleccion_guardada() {
        let encontrados = vec![PathBuf::from("/p/orion"), PathBuf::from("/p/lab")];
        let guardados = vec![PathBuf::from("/p/orion")];

        let lista = marcar_seleccion(encontrados, &guardados);

        assert_eq!(lista.len(), 2);
        assert!(lista.iter().find(|p| p.nombre == "orion").unwrap().seleccionado);
        assert!(!lista.iter().find(|p| p.nombre == "lab").unwrap().seleccionado);
    }

    /// Review Focus #4: un proyecto guardado que ya no existe no debe romper
    /// el arranque ni llevarse por delante al resto de la selección.
    #[test]
    fn un_proyecto_guardado_que_desaparecio_se_ignora_sin_perder_los_demas() {
        let encontrados = vec![PathBuf::from("/p/orion")];
        let guardados = vec![PathBuf::from("/p/borrado"), PathBuf::from("/p/orion")];

        let lista = marcar_seleccion(encontrados, &guardados);

        assert_eq!(lista.len(), 1, "solo se listan los que existen");
        assert!(lista[0].seleccionado, "el que sí existe sigue seleccionado");
    }

    #[test]
    fn la_lista_va_ordenada_por_nombre_para_ser_predecible() {
        let encontrados = vec![
            PathBuf::from("/p/zzz"),
            PathBuf::from("/p/aaa"),
            PathBuf::from("/p/mmm"),
        ];

        let nombres: Vec<String> = marcar_seleccion(encontrados, &[])
            .into_iter()
            .map(|p| p.nombre)
            .collect();

        assert_eq!(nombres, vec!["aaa", "mmm", "zzz"]);
    }
}
```

- [ ] **Step 2: Ejecutar los tests para verificar que fallan**

Añadir `mod selector;` a `main.rs`.

Run: `cargo test -p argos-gui selector`
Expected: FAIL, `marcar_seleccion` no definida.

- [ ] **Step 3: Implementar la parte pura**

Al principio de `crates/argos-gui/src/selector.rs`:

```rust
use std::path::PathBuf;

pub struct ProyectoDisponible {
    pub path: PathBuf,
    pub nombre: String,
    pub seleccionado: bool,
}

/// Cruza lo que hay en disco con lo que el usuario tenía elegido. Un proyecto
/// guardado que ya no existe simplemente no aparece: ignorarlo es más útil que
/// mostrar una entrada muerta o vaciar la selección entera.
pub fn marcar_seleccion(
    encontrados: Vec<PathBuf>,
    guardados: &[PathBuf],
) -> Vec<ProyectoDisponible> {
    let mut lista: Vec<ProyectoDisponible> = encontrados
        .into_iter()
        .map(|path| ProyectoDisponible {
            nombre: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned()),
            seleccionado: guardados.contains(&path),
            path,
        })
        .collect();

    lista.sort_by(|a, b| a.nombre.cmp(&b.nombre));
    lista
}
```

- [ ] **Step 4: Ejecutar los tests para verificar que pasan**

Run: `cargo test -p argos-gui selector`
Expected: PASS, 3 tests.

- [ ] **Step 5: Conectar la UI al `Watcher` y al selector**

La pantalla de monitoreo **conserva el desglose de dos niveles que ya existe**
(proyectos → ramas, con `summarize_projects` y `group_by_branch`), solo que ahora
contiene únicamente lo seleccionado. Con un solo proyecto elegido verás una sola
entrada; con varios, los verás agrupados.

En `crates/argos-gui/src/app.rs`, sustituir el `Monitor` directo por el `Watcher`:

```rust
use crate::selector::{marcar_seleccion, ProyectoDisponible};
use argos_core::discovery::find_repos;
use argos_core::monitor::MonitorConfig;
use argos_core::scope::Scope;
use argos_core::store::Store;
use argos_core::watcher::{EstadoSondeo, Watcher};

#[derive(PartialEq, Clone, Copy)]
pub enum Pantalla {
    Selector,
    Monitoreo,
}

pub struct ArgosApp {
    watcher: Watcher,
    store: Option<Store>,
    snapshot: Option<Snapshot>,
    pantalla: Pantalla,
    disponibles: Vec<ProyectoDisponible>,
    pub selected: Option<String>,
    pub filter: Filter,
    pub abierto: Option<Option<PathBuf>>,
}

impl ArgosApp {
    pub fn new() -> Self {
        let config = MonitorConfig::default();
        let store = Store::open(&config.db_path).ok();

        // Una selección guardada que ya no existe en disco se ignora sola:
        // `marcar_seleccion` solo lista lo que encontró.
        let guardados = store
            .as_ref()
            .and_then(|s| s.watched().ok())
            .unwrap_or_default();

        let encontrados: Vec<PathBuf> = config
            .search_roots
            .iter()
            .flat_map(|r| find_repos(r, config.max_depth))
            .collect();
        let disponibles = marcar_seleccion(encontrados, &guardados);

        let seleccion: Vec<PathBuf> = disponibles
            .iter()
            .filter(|p| p.seleccionado)
            .map(|p| p.path.clone())
            .collect();

        let pantalla = if seleccion.is_empty() {
            Pantalla::Selector
        } else {
            Pantalla::Monitoreo
        };

        let config = MonitorConfig {
            scope: Scope::projects(seleccion),
            ..config
        };

        ArgosApp {
            watcher: Watcher::start(config, REFRESH),
            store,
            snapshot: None,
            pantalla,
            disponibles,
            selected: None,
            filter: Filter::default(),
            abierto: None,
        }
    }

    fn aplicar_seleccion(&mut self) {
        let seleccion: Vec<PathBuf> = self
            .disponibles
            .iter()
            .filter(|p| p.seleccionado)
            .map(|p| p.path.clone())
            .collect();

        if let Some(store) = &self.store {
            let _ = store.save_watched(&seleccion);
        }

        self.watcher.set_scope(Scope::projects(seleccion));
        self.abierto = None;
        self.selected = None;
        self.pantalla = Pantalla::Monitoreo;
    }

    fn pintar_selector(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("¿Qué proyectos quieres monitorear?");
            ui.weak("Argos solo leerá los logs de los proyectos que elijas.");
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                for p in &mut self.disponibles {
                    ui.checkbox(&mut p.seleccionado, &p.nombre);
                }
            });

            ui.separator();
            let elegidos = self.disponibles.iter().filter(|p| p.seleccionado).count();

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(elegidos > 0, egui::Button::new("Monitorear"))
                    .clicked()
                {
                    self.aplicar_seleccion();
                }
                ui.weak(format!("{elegidos} seleccionado(s)"));
            });
        });
    }
}
```

Y en `update`, leer el último resultado **sin esperar**:

```rust
impl eframe::App for ArgosApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // No bloquea: si el hilo aún no publicó nada, seguimos con lo último
        // que teníamos (o con nada, la primera vez).
        if let Some(s) = self.watcher.latest() {
            self.snapshot = Some(s);
        }
        ctx.request_repaint_after(REFRESH);

        let sesiones = self.snapshot.as_ref().map(|s| s.rows.len()).unwrap_or(0);

        egui::TopBottomPanel::top("encabezado").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Argos");

                if self.pantalla == Pantalla::Monitoreo {
                    if ui.button("Proyectos").clicked() {
                        self.pantalla = Pantalla::Selector;
                    }
                    ui.label(format!("{sesiones} sesiones"));
                }

                if self.watcher.estado() == EstadoSondeo::Detenido {
                    ui.colored_label(
                        egui::Color32::from_rgb(210, 90, 90),
                        "el sondeo se detuvo: los datos no se están actualizando",
                    );
                }

                if let Some(err) = self.snapshot.as_ref().and_then(|s| s.persist_error.as_ref()) {
                    ui.colored_label(
                        egui::Color32::from_rgb(210, 90, 90),
                        format!("sin guardar histórico: {err}"),
                    );
                }

                if self.pantalla == Pantalla::Monitoreo {
                    ui.separator();
                    ui.selectable_value(&mut self.filter, Filter::All, "Todas");
                    ui.selectable_value(&mut self.filter, Filter::NeedsAttention, "Me esperan");
                    ui.selectable_value(&mut self.filter, Filter::Active, "Activas");
                }
            });
        });

        if self.pantalla == Pantalla::Selector {
            self.pintar_selector(ctx);
            return;
        }

        let Some(snapshot) = self.snapshot.clone() else {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.weak("Sondeando…");
            });
            return;
        };

        // A partir de aquí, el resto de la vista igual que hoy pero usando
        // `snapshot` en vez de `self.snapshot`.
        self.pintar_monitoreo(ctx, &snapshot);
    }
}
```

Renombrar los métodos existentes `pintar_proyectos` y `pintar_ramas` para que reciban
`&Snapshot` en vez de leer `self.snapshot`, y añadir `pintar_monitoreo` que despacha
entre los dos según `self.abierto`:

```rust
    fn pintar_monitoreo(&mut self, ctx: &egui::Context, snapshot: &Snapshot) {
        match self.abierto.clone() {
            None => self.pintar_proyectos(ctx, snapshot),
            Some(project) => self.pintar_ramas(ctx, project, snapshot),
        }
    }
```

- [ ] **Step 6: Medir la mejora real**

Con Argos abierto y un solo proyecto seleccionado, comprobar a mano:

1. La ventana responde al instante al cambiar de filtro o abrir un grupo, sin trabones.
2. El primer resultado aparece en menos de un segundo.
3. Seleccionar tres proyectos y volver a comprobar que sigue fluida.

Y medir el ciclo acotado:

```bash
time ./target/release/argos-probe > /dev/null   # sigue en Scope::all(), ~1.6 s
```

Expected: la ventana ya no se congela, porque el sondeo no corre en su hilo. El número de `argos-probe` no cambia y eso está bien: sigue siendo el diagnóstico completo.

- [ ] **Step 7: Verificación completa**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check`
Expected: PASS, sin advertencias.

- [ ] **Step 8: Commit**

```bash
git add crates
git commit -m "feat: selector de proyectos con selección múltiple y vista acotada"
```
