use crate::error::ProbeError;
use crate::model::ClientKind;
use crate::observation::ProcessObservation;
use chrono::{DateTime, Utc};
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct WarpIdentity {
    pub session_uuid: Option<String>,
    pub focus_url: Option<String>,
}

/// Procesos auxiliares que comparten nombre con un CLI de agente pero no son
/// sesiones. Verificado en máquina: `claude --chrome-native-host`.
const AUXILIARY_FLAGS: [&str; 1] = ["--chrome-native-host"];

pub fn client_from_command(command: &str) -> Option<ClientKind> {
    if AUXILIARY_FLAGS.iter().any(|flag| command.contains(flag)) {
        return None;
    }

    let executable = command.split_whitespace().next()?;
    let name = executable.rsplit('/').next()?;

    ClientKind::ALL
        .into_iter()
        .find(|c| c.process_name() == name)
}

pub fn parse_warp_env(env: &str) -> WarpIdentity {
    let mut identity = WarpIdentity::default();

    for token in env.split_whitespace() {
        if let Some(value) = token.strip_prefix("WARP_TERMINAL_SESSION_UUID=") {
            identity.session_uuid = Some(value.to_string());
        } else if let Some(value) = token.strip_prefix("WARP_FOCUS_URL=") {
            identity.focus_url = Some(value.to_string());
        }
    }

    identity
}

/// `lsof` pone la ruta al final de la línea y las rutas pueden llevar espacios,
/// así que se toma todo lo que sigue a la columna NODE, no el último campo.
pub fn parse_lsof_cwd(output: &str) -> Option<PathBuf> {
    for line in output.lines().skip(1) {
        let mut fields = line.split_whitespace();
        // COMMAND PID USER FD TYPE DEVICE SIZE/OFF NODE → 8 columnas antes de NAME
        let consumed: Vec<&str> = fields.by_ref().take(8).collect();
        if consumed.len() < 8 {
            continue;
        }
        let rest: Vec<&str> = fields.collect();
        if rest.is_empty() {
            continue;
        }
        return Some(PathBuf::from(rest.join(" ")));
    }
    None
}

pub struct ProcessProbe;

impl ProcessProbe {
    pub fn new() -> Self {
        ProcessProbe
    }

    pub fn observe(&self) -> Result<Vec<ProcessObservation>, ProbeError> {
        let output = Command::new("ps")
            .args(["-Ao", "pid=,ppid=,lstart=,command="])
            .output()
            .map_err(|source| ProbeError::Command {
                command: "ps".into(),
                source,
            })?;

        let listing = String::from_utf8_lossy(&output.stdout);
        let mut observations = Vec::new();

        for line in listing.lines() {
            let Some(observation) = self.parse_process_line(line) else {
                continue;
            };
            observations.push(observation);
        }

        Ok(observations)
    }

    fn parse_process_line(&self, line: &str) -> Option<ProcessObservation> {
        let mut fields = line.split_whitespace();
        let pid: u32 = fields.next()?.parse().ok()?;
        let ppid: u32 = fields.next()?.parse().ok()?;

        // lstart ocupa 5 campos: "Sat Sep 19 20:37:42 2026"
        let lstart: Vec<&str> = fields.by_ref().take(5).collect();
        if lstart.len() < 5 {
            return None;
        }

        let command: String = fields.collect::<Vec<_>>().join(" ");
        let client = client_from_command(&command)?;
        let warp = warp_identity_of(pid);

        Some(ProcessObservation {
            pid,
            ppid,
            client,
            cwd: cwd_of(pid),
            started_at: parse_lstart(&lstart.join(" ")).unwrap_or_else(Utc::now),
            warp_session_uuid: warp.session_uuid,
            warp_focus_url: warp.focus_url,
        })
    }
}

impl Default for ProcessProbe {
    fn default() -> Self {
        Self::new()
    }
}

/// `ps` imprime lstart como "Sat Sep 19 20:37:42 2026" en hora local.
fn parse_lstart(raw: &str) -> Option<DateTime<Utc>> {
    use chrono::NaiveDateTime;
    let naive = NaiveDateTime::parse_from_str(raw.trim(), "%a %b %e %H:%M:%S %Y").ok()?;
    Some(naive.and_utc())
}

fn cwd_of(pid: u32) -> Option<PathBuf> {
    let output = Command::new("lsof")
        .args(["-a", "-d", "cwd", "-p", &pid.to_string()])
        .output()
        .ok()?;
    parse_lsof_cwd(&String::from_utf8_lossy(&output.stdout))
}

fn warp_identity_of(pid: u32) -> WarpIdentity {
    let Ok(output) = Command::new("ps")
        .args(["-Ewwo", "command=", "-p", &pid.to_string()])
        .output()
    else {
        return WarpIdentity::default();
    };
    parse_warp_env(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ClientKind;

    #[test]
    fn identifica_el_cliente_por_nombre_de_proceso() {
        assert_eq!(client_from_command("claude"), Some(ClientKind::ClaudeCode));
        assert_eq!(
            client_from_command("/usr/local/bin/codex"),
            Some(ClientKind::Codex)
        );
        assert_eq!(
            client_from_command("/Users/x/.local/bin/agy"),
            Some(ClientKind::Antigravity)
        );
        assert_eq!(client_from_command("zsh"), None);
    }

    #[test]
    fn no_confunde_procesos_auxiliares_con_sesiones_de_agente() {
        // Observado en la máquina real: el host nativo de Chrome de Claude Code
        // es un proceso `claude` que NO es una sesión de agente.
        assert_eq!(
            client_from_command(
                "/opt/homebrew/Caskroom/claude-code/2.1.236/claude --chrome-native-host"
            ),
            None
        );
    }

    #[test]
    fn extrae_los_identificadores_de_warp_del_entorno() {
        let env = "PATH=/usr/bin WARP_TERMINAL_SESSION_UUID=15afe4b093924586a06cdcced8e499dd \
                   WARP_FOCUS_URL=warp://session/15afe4b093924586a06cdcced8e499dd TERM=xterm";
        let warp = parse_warp_env(env);
        assert_eq!(
            warp.session_uuid.as_deref(),
            Some("15afe4b093924586a06cdcced8e499dd")
        );
        assert_eq!(
            warp.focus_url.as_deref(),
            Some("warp://session/15afe4b093924586a06cdcced8e499dd")
        );
    }

    #[test]
    fn un_proceso_fuera_de_warp_no_tiene_identificadores_pero_no_falla() {
        let warp = parse_warp_env("PATH=/usr/bin TERM=xterm SHELL=/bin/zsh");
        assert_eq!(warp.session_uuid, None);
        assert_eq!(warp.focus_url, None);
    }

    #[test]
    fn extrae_el_cwd_de_la_salida_de_lsof() {
        let salida = "COMMAND   PID   USER   FD   TYPE DEVICE SIZE/OFF      NODE NAME\n\
                      claude  24569  alex    cwd    DIR   1,16      768 199091204 /Users/alex/Proyectos\n";
        assert_eq!(
            parse_lsof_cwd(salida),
            Some(std::path::PathBuf::from("/Users/alex/Proyectos"))
        );
    }

    #[test]
    fn un_cwd_con_espacios_se_conserva_completo() {
        let salida = "COMMAND PID USER FD TYPE DEVICE SIZE/OFF NODE NAME\n\
                      codex 42 alex cwd DIR 1,16 768 123 /Users/alex/Mis Proyectos/app\n";
        assert_eq!(
            parse_lsof_cwd(salida),
            Some(std::path::PathBuf::from("/Users/alex/Mis Proyectos/app"))
        );
    }

    /// No asevera nada sobre el contenido: la máquina puede no tener agentes
    /// corriendo. Verifica que la recolección real no entra en pánico.
    #[test]
    fn la_recoleccion_real_no_entra_en_panico() {
        let probe = ProcessProbe::new();
        let resultado = probe.observe();
        assert!(resultado.is_ok(), "observe() falló: {resultado:?}");
    }
}
