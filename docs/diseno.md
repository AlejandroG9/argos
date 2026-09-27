# Argos — identidad visual

## La marca

Argos Panoptes, el gigante de cien ojos que todo lo vigilaba. La marca es un **ojo cuyo
iris es un grafo**: un nodo central del que salen radios hacia ocho nodos en anillo.

Dice las dos cosas a la vez — lo que el nombre significa y lo que la aplicación dibuja.
No fue una metáfora buscada: la app literalmente pinta nodos conectados, así que el
grafo *es* el producto y el ojo *es* el nombre.

**Sin texto.** A 16 píxeles cualquier palabra se vuelve una mancha, y el icono de una app
de escritorio vive casi siempre por debajo de 64.

### Dos versiones, no una

| Archivo | Uso | Por qué |
|---|---|---|
| `assets/argos-marca.svg` | 64 px en adelante | Los ocho vigías y el anillo completo |
| `assets/argos-marca-pequena.svg` | 16 y 32 px | Cuatro nodos, trazos gruesos |
| `assets/argos-icono.svg` | El icono del sistema | La marca **metida** en la baldosa |

Ocho nodos se empastan por debajo de 48 píxeles: el anillo se convierte en una aureola
gris y el conjunto en un borrón. La variante pequeña baja a cuatro nodos y engorda los
trazos, así que conserva la silueta reconocible en vez de degradarse.

**El icono no es la marca a secas.** La marca llena su lienzo; un icono que llena el
suyo se ve enorme en el Dock, porque todos los demás dejan margen. `argos-icono.svg`
pone la baldosa de 824 px dentro de un lienzo de 1024 con el radio de esquina de Big
Sur, y escala el grafo al 0.805 para conservar las proporciones. Es lo que alimenta
tanto el `.icns` como el icono de la ventana (`argos-icono.png`, empotrado en el
binario).

`assets/Argos.icns` combina ambas siluetas por tamaño: la de cuatro nodos a 16 y 32, la
completa de ahí en adelante. Se genera con `iconutil -c icns <iconset> -o
assets/Argos.icns`.

## Los tipos

| Familia | Tipo | Papel |
|---|---|---|
| display | Instrument Serif | "Argos" y los títulos de pantalla |
| cuerpo | IBM Plex Sans | Todo lo que se lee |
| `fuerte` | IBM Plex Sans SemiBold | Encabezados: la jerarquía la hace el peso |
| mono | IBM Plex Mono | Shas, rutas, hexadecimales |

Viajan en el binario desde `assets/fonts/` (son OFL; las licencias están al lado). Sin
empotrarlos, egui usa su tipo por defecto y la app se parece a cualquier otra app de
egui en vez de a su propio diseño. Se instalan una sola vez al arrancar
(`tipografia::instalar`): cada cambio reconstruye el atlas de glifos.

Los tipos de egui quedan **detrás** de los nuestros en cada familia, no sustituidos:
la interfaz usa ◆ ▶ ✓ ✕ ← y no todos están en Plex. Lo que falte cae en el respaldo en
lugar de salir como un cuadrito.

## La paleta

Un color tiene un papel, no un gusto. Estos son los papeles:

### Superficie

| Nombre | Hex | Papel |
|---|---|---|
| `FONDO` | `#14161A` | El lienzo. Todo lo demás flota encima |
| `SUPERFICIE` | `#1C1F26` | Barras y tarjetas: lo que **no** es contenido |
| `SUPERFICIE_ALTA` | `#262A33` | Reposo del cursor y selección |
| `BORDE` | `#2E333D` | Separaciones de un píxel |
| `LINEA` | `#393F4B` | Aristas del grafo |

Tres niveles de gris bastan. Un cuarto solo añade decisiones sin añadir información.

### Texto

| Nombre | Hex | Papel |
|---|---|---|
| `TEXTO` | `#E4E7EC` | Lo que hay que leer |
| `TEXTO_TENUE` | `#8B93A1` | Lo que acompaña: fechas, rutas, conteos |

Dos niveles, no cinco. La jerarquía la hace el tamaño y el peso, no un degradado de
grises que nadie distingue.

### Marca y acento

Son **dos colores con dos trabajos**, y ahí está toda la regla:

| Nombre | Hex | Papel |
|---|---|---|
| `MARCA` | `#B87333` cobre | Solo identidad: el icono, el `.icns`, la marca de la barra |
| `ACENTO` | `#E8E3D7` hueso | Solo cromo: chip activo, selección, foco |
| `SOBRE_ACENTO` | `#0F1114` | El texto encima de un relleno de acento |

**Por qué están separados.** El cobre comparte familia de tono con el ámbar de
*esperando*. Como cromo interactivo competiría con él —el chip y la insignia de un
agente que te espera acaban a dos centímetros, y el ojo tarda en decidir cuál es un
botón y cuál un aviso—. Como identidad no llega a tocarlo nunca: el icono vive en el
Dock y la marca vive en la barra de la pantalla de selección, donde no hay estados.

**Por qué el cromo es acromático.** Sin tono no se parece a ningún estado, ni ahora ni
cuando se añada un séptimo carril. El hueso es la única elección que no hay que volver
a revisar cada vez que la paleta de datos crece.

Las dos reglas están clavadas en tests (`theme.rs`): el acento se separa de los cuatro
colores de estado en luminancia, y `MARCA != ACENTO`. Igualarlos reintroduciría el
choque sin que nadie lo note.

### Estado

Estos cuatro **no son decorativos**: son la información principal de la aplicación.

| Estado | Hex | Símbolo |
|---|---|---|
| Esperando respuesta | `#E6A01E` ámbar | ◆ |
| Trabajando | `#3CAA6E` verde | ▶ |
| Desconocido | `#8C8C96` gris | ? |
| Terminó | `#5F6E82` azul apagado | ✓ |

**Cada estado lleva símbolo además de color, siempre.** Quien no distingue rojo de verde
—entre el 4 % y el 8 % de los hombres— seguiría usando la app sin perder nada. Un
indicador que depende solo del color no es un indicador, es una decoración.

El gris y el azul apagado de los dos últimos estados son deliberadamente sosos: lo
terminado y lo desconocido no deben competir por la atención con lo que está vivo.

### Carriles del grafo

Seis colores que se repiten al agotarse. Más de seis ramas simultáneas dejan de
distinguirse por color por muchos que añadas, así que añadir un séptimo sería fingir una
precisión que el ojo no tiene.

## Espaciado

`4 · 8 · 12 · 16 · 24`. Usar siempre estos valores y nunca un número suelto es la mitad
de por qué una interfaz se ve ordenada. La otra mitad es la restricción: pocos elementos
compitiendo a la vez.

## Tipografía

| Estilo | Tamaño | Uso |
|---|---|---|
| Heading | 19 | Nombre del proyecto |
| Body | 13.5 | Texto general |
| Button | 13 | Controles |
| Small | 11.5 | Etiquetas de apoyo |
| Monospace | 12.5 | SHA de commits |

Los identificadores van en monoespaciada: un sha en proporcional se lee peor y se compara
peor, que es justo lo que se hace con un sha.

## Lo que no hacemos

**No incrustamos logotipos de terceros.** Claude, Codex y Gemini son marcas de Anthropic,
OpenAI y Google. La app muestra una inicial, y usa los logotipos reales solo si el usuario
los pone en `~/.argos/logos/`. La decisión es suya, no nuestra.

**No animamos lo que no se mueve.** El repintado continuo se activa solo cuando hay un
agente activo. Una app que vive abierta en segundo plano no puede gastar CPU animando un
árbol quieto.

**No afirmamos con confianza lo que inferimos.** Una correlación dudosa se muestra dudosa
—con `~` o `?` junto al estado— en vez de presentarse como un hecho.
