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

Ocho nodos se empastan por debajo de 48 píxeles: el anillo se convierte en una aureola
gris y el conjunto en un borrón azul. La variante pequeña baja a cuatro nodos y engorda
los trazos, así que conserva la silueta reconocible en vez de degradarse.

`assets/Argos.icns` ya combina ambas por tamaño. Se genera con:

```bash
iconutil -c icns /tmp/Argos.iconset -o assets/Argos.icns
```

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

### Marca

| Nombre | Hex | Papel |
|---|---|---|
| `ACENTO` | `#6C8CF5` | La marca. Selección activa y foco |

**El acento es azul por una razón concreta, no estética:** el ámbar y el verde ya
significan algo —esperando y trabajando— y usarlos para la identidad haría que la marca
compitiera con el estado. El azul se queda fuera del espacio semántico.

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
