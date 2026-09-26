use chrono::{DateTime, Duration, Local, Utc};

/// Ventana temporal de la vista. Los nombres dicen lo que hacen: *Hoy* es el
/// día de calendario, y las otras dos son ventanas rodantes. Llamarlas
/// "semana" y "mes" sería ambiguo — un lunes a las 9am "esta semana" con
/// semántica de calendario no mostraría casi nada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Ventana {
    Hoy,
    Dias7,
    Dias30,
    #[default]
    Todo,
}

impl Ventana {
    pub fn etiqueta(self) -> &'static str {
        match self {
            Ventana::Hoy => "Hoy",
            Ventana::Dias7 => "7 días",
            Ventana::Dias30 => "30 días",
            Ventana::Todo => "Todo",
        }
    }

    pub fn acepta(self, last_activity: DateTime<Utc>, now: DateTime<Utc>) -> bool {
        // Una fecha futura viene de un desfase de reloj, no de un dato viejo.
        // Mostrarla de más es preferible a esconderla.
        if last_activity > now {
            return true;
        }

        match self {
            Ventana::Todo => true,
            Ventana::Hoy => last_activity >= medianoche_local(now),
            Ventana::Dias7 => last_activity >= now - Duration::days(7),
            Ventana::Dias30 => last_activity >= now - Duration::days(30),
        }
    }
}

/// Medianoche del día local que contiene `now`, expresada en UTC.
fn medianoche_local(now: DateTime<Utc>) -> DateTime<Utc> {
    now.with_timezone(&Local)
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|naive| naive.and_local_timezone(Local).single())
        .map(|local| local.with_timezone(&Utc))
        // Sin medianoche resoluble (cambio de horario raro), no escondemos nada.
        .unwrap_or(DateTime::<Utc>::MIN_UTC)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ahora() -> DateTime<Utc> {
        // Media tarde local, para que "hoy" tenga margen por ambos lados.
        Local::now()
            .date_naive()
            .and_hms_opt(15, 0, 0)
            .unwrap()
            .and_local_timezone(Local)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn todo_acepta_cualquier_antiguedad() {
        let now = ahora();
        assert!(Ventana::Todo.acepta(now - Duration::days(9999), now));
    }

    #[test]
    fn hoy_acepta_lo_de_hace_un_rato() {
        let now = ahora();
        assert!(Ventana::Hoy.acepta(now - Duration::minutes(5), now));
    }

    /// El punto de llamarlo "Hoy" y no "24 horas": a las 3pm, lo de ayer a
    /// las 4pm no es de hoy aunque hayan pasado menos de 24 horas.
    #[test]
    fn hoy_rechaza_lo_de_ayer_aunque_no_hayan_pasado_24_horas() {
        let now = ahora();
        let ayer_por_la_tarde = now - Duration::hours(23);

        assert!(
            !Ventana::Hoy.acepta(ayer_por_la_tarde, now),
            "cruzó la medianoche, así que no es de hoy"
        );
        assert!(
            Ventana::Dias7.acepta(ayer_por_la_tarde, now),
            "pero sí entra en la ventana rodante"
        );
    }

    #[test]
    fn hoy_acepta_justo_despues_de_medianoche() {
        let now = ahora();
        assert!(Ventana::Hoy.acepta(medianoche_local(now), now));
    }

    #[test]
    fn las_ventanas_rodantes_cortan_donde_dicen() {
        let now = ahora();

        assert!(Ventana::Dias7.acepta(now - Duration::days(6), now));
        assert!(
            Ventana::Dias7.acepta(now - Duration::days(7), now),
            "el borde exacto entra"
        );
        assert!(!Ventana::Dias7.acepta(now - Duration::days(8), now));

        assert!(Ventana::Dias30.acepta(now - Duration::days(29), now));
        assert!(!Ventana::Dias30.acepta(now - Duration::days(31), now));
    }

    /// Un desfase de reloj no debe hacer desaparecer una sesión activa.
    #[test]
    fn una_fecha_futura_se_muestra_en_vez_de_esconderse() {
        let now = ahora();
        let futuro = now + Duration::hours(2);

        assert!(Ventana::Hoy.acepta(futuro, now));
        assert!(Ventana::Dias7.acepta(futuro, now));
    }
}
