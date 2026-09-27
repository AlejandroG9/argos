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

pub fn filtrar_proyectos<'a>(
    proyectos: &'a [ProyectoDisponible],
    consulta: &str,
) -> Vec<&'a ProyectoDisponible> {
    let consulta = consulta.trim().to_lowercase();
    if consulta.is_empty() {
        return proyectos.iter().collect();
    }

    proyectos
        .iter()
        .filter(|proyecto| {
            proyecto.nombre.to_lowercase().contains(&consulta)
                || proyecto
                    .path
                    .to_string_lossy()
                    .to_lowercase()
                    .contains(&consulta)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marca_como_seleccionados_los_que_vienen_de_la_seleccion_guardada() {
        let encontrados = vec![PathBuf::from("/p/orion"), PathBuf::from("/p/lab")];
        let guardados = vec![PathBuf::from("/p/orion")];

        let lista = marcar_seleccion(encontrados, &guardados);

        assert_eq!(lista.len(), 2);
        assert!(
            lista
                .iter()
                .find(|p| p.nombre == "orion")
                .unwrap()
                .seleccionado
        );
        assert!(
            !lista
                .iter()
                .find(|p| p.nombre == "lab")
                .unwrap()
                .seleccionado
        );
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

    #[test]
    fn la_busqueda_encuentra_por_nombre_sin_importar_mayusculas() {
        let proyectos = marcar_seleccion(
            vec![PathBuf::from("/p/Orion"), PathBuf::from("/p/Laboratorio")],
            &[],
        );

        let encontrados = filtrar_proyectos(&proyectos, "ORIon");

        assert_eq!(encontrados.len(), 1);
        assert_eq!(encontrados[0].nombre, "Orion");
    }

    #[test]
    fn la_busqueda_tambien_encuentra_por_ruta() {
        let proyectos = marcar_seleccion(
            vec![
                PathBuf::from("/Users/alex/Proyectos/orion"),
                PathBuf::from("/Users/alex/Clientes/acme/app"),
            ],
            &[],
        );

        let encontrados = filtrar_proyectos(&proyectos, "clientes/acme");

        assert_eq!(encontrados.len(), 1);
        assert_eq!(encontrados[0].nombre, "app");
    }

    #[test]
    fn una_busqueda_vacia_conserva_todos_los_proyectos() {
        let proyectos = marcar_seleccion(
            vec![PathBuf::from("/p/orion"), PathBuf::from("/p/lab")],
            &[],
        );

        assert_eq!(filtrar_proyectos(&proyectos, "   ").len(), 2);
    }
}
