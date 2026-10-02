# Planète de glace : le kit de blocs

Le kit de blocs de la planète de glace, tenu à part de `docs/blocks.md` (Mars) pour ne pas
mélanger les deux. Il est publié dans l'onglet « Glace » de la page partagée :
https://claude.ai/artifact/QNgJoQ6GnMenV8nMVZ1USZ

## Les matières

Sur la planète de glace (`planet ice`), les surfaces du kit sont d'autres matières, et la voiture
à skis (`physics::neige`) les conduit à sa façon :

| Bloc | Devient | Pour la voiture |
|---|---|---|
| tablier de route | glace vive, bleue et brillante | peu d'adhérence, glisse jusqu'à 45°, la trajectoire suit le nez avec retard |
| `dirt` | neige tassée, 28 m | les roues s'enfoncent de 10 cm, le moteur tire environ moitié moins que sur la glace (≈ 225 km/h au plus), glisses courtes et chères, traces |
| `snow` | la même neige, **16 m** | pareil, sur une piste étroite : la neige est plus lente que la glace, ses parties sont serrées |
| sol | poudreuse | les roues s'enfoncent de 22 cm, la voiture laboure : freinée avec le carré de sa vitesse vers ≈ 110 km/h, vite quand elle sort à pleine vitesse |

Les bords (sacs, boudins), les pilotis et les portiques sont encore ceux de Mars, en couleurs
unies.

## Les blocs de la glace

Ce que le jeu en apprend (2 octobre 2026) : **la glace se joue comme une voiture de drift**, en
glisse tout le long. Les quarts de virage secs du kit de Mars (rayon 16 m) cassent ce rythme ;
ces blocs le suivent.

| Bloc | Ce que c'est |
|---|---|
| `curveN_left/right` | virage progressif sur `N × N` cellules (N = 2 à 4), entre les mêmes raccords qu'un `turnN` : la courbure monte doucement jusqu'à l'apex puis retombe (k ∝ sin²), droit aux deux bouts. On entre en glisse, on la tient à l'apex, on la déroule en sortie. |
| `curvebermN_left/right` | le même, l'extérieur relevé de 18° : là où la glace doit tourner serré, c'est la piste qui la fait tourner |
| `sbendN_left/right` | un S qui décale la piste d'une cellule sur `N` cellules (N = 2 à 4) : il fait basculer une glisse d'un côté à l'autre |

| Bloc | Longueur | Rayon le plus serré |
|---|---|---|
| `curve2` | 82 m | 26 m à l'apex |
| `curve3` | 137 m | 43 m |
| `curve4` | 192 m | 59 m |
| `sbend2` | 64 m | 26 m |
| `sbend3` | 96 m | 54 m |
| `sbend4` | 128 m | 93 m |

Pour comparer, la voiture à skis tourne à 16 m de rayon à fond de braquage à 120 km/h sur la
glace, à 22 m à 200 km/h. Règle de construction : **jamais de virage serré à plat sur la glace**
(un `turn1` y reste possible, mais il casse la glisse).

## Les virages qui montent et descendent

Les virages du kit peuvent monter ou descendre d'un ou deux niveaux en tournant, avec le profil
arrondi des pentes (plat aux deux bouts) : le suffixe `_upL` ou `_downL`, sur `turnN`, `bermN`,
`ubermN`, `curveN` et `curvebermN`, en glace comme en neige.

| Exemple | Ce que c'est |
|---|---|
| `curve3_left_down2` | un virage progressif qui descend de 16 m : on y lance une glisse en prenant de la vitesse |
| `curveberm3_left_up1` | un virage progressif relevé qui monte de 8 m : il ralentit la voiture avant une partie lente |
| `uberm2_left_up1` + `snow` | une épingle de neige relevée qui monte d'un niveau |

Le kit mesure le profil de chaque virage qui monte et refuse ceux qui feraient une bosse : aucun
sommet plus serré que 55 m de rayon (la voiture reste au sol jusqu'à 170 km/h), aucun creux plus
serré que 25 m. Un `turn1` ou un `uberm1` ne montent donc pas, et un `berm2` monte d'un niveau
au plus (son dévers et sa montée s'ajoutent).

## Les blocs de la neige

La neige est plus lente que la glace (le moteur y tire environ moitié moins que sur la glace,
≈ 225 km/h au plus) : les couloirs de 28 m de Mars y étaient trop larges. Une piste de neige se construit avec la variante **`snow`** (16 m de large)
et ces blocs :

| Bloc | Ce que c'est |
|---|---|
| `snakeN` + `snow` | `N` cellules de virages enchaînés, un par cellule, à gauche puis à droite, de 4 m de creux (rayon 11 m) : la piste serpente dans sa colonne |
| `sbend2_left/right` + `snow` | une chicane : la piste passe d'une colonne à la voisine sur deux cellules |
| `turn1`, `uberm1` + `snow` | les épingles du kit de Mars, sur la piste étroite |
| `to_dirt snow`, `to_road snow` | le passage de la glace à la neige et retour : la glace garde ses 20 m jusqu'à mi-cellule, la neige se resserre à 16 m dans son couloir |

Règle de construction : deux portions de neige parallèles doivent garder **une cellule libre**
entre elles, sinon leurs couloirs se rejoignent en une seule grande étendue et la piste paraît
large de nouveau.

## Le premier circuit : Sulcus

`crates/track/maps/sulcus.chain` (version 2), série facile, ≈ 1,5 km, 28,4 à 33 s au pilote
d'essai (or 27,3 s). La première version, toute plate, faisait se suivre lignes droites et
virages à 90° : celle-ci monte et descend, et ses virages serrés tombent là où la voiture est
lente.

- **Départ** sur une mesa, 24 m plus haut : un virage serré tout de suite, à basse vitesse, puis
  un `curve3_left_down2` qui descend de deux niveaux dans une tranchée, la vitesse montant dans la
  glisse, et une pente jusqu'à la vallée.
- **Glace rapide** dans la vallée : un `curveberm3` large et relevé, un long `sbend4` à pleine
  vitesse, puis un `curveberm3_left_up1` qui remonte d'un niveau et ralentit la voiture avant la
  neige.
- **Neige** (16 m) sur un plateau : un virage relevé, une montée sur le plateau, un long
  `curveberm2` relevé, un seul serpentin, un `curve2_left_down1` qui descend du plateau, une
  chicane, une épingle et un `curveberm2_left_down1` relevé qui descend jusqu'à la plaine.
- **Glace** : un `sbend3` jusqu'à l'arrivée.

Règles suivies : les virages serrés là où la voiture est lente (au départ, dans la neige, après
une montée), pas juste après une pointe de vitesse ; pas de neige parallèle à elle-même sans une
cellule libre entre les deux. Pas de camp de Mars autour : la mesa du départ, le plateau de la
neige, quelques buttes et des falaises, en attendant les décors de la planète. La montée vers la
neige passe encore sur les pilotis de Mars.

## Planche des formes

Dessinée depuis la géométrie du jeu par `cargo run -p track --release --example shapes`, dans
`docs/blocks-ice/` : les virages progressifs (à plat et relevés), les S, les serpentins, le
passage à la neige, la chicane et l'épingle en neige, et des virages qui montent et descendent.

## À faire

- Des gouttières de bobsleigh pour la glace : un profil en U où l'on choisit sa hauteur sur la
  paroi.
- Un jeu de décors propre à cette planète, sans les camps ni la colonie de Mars.
- Des bords, des pilotis et des portiques à elle, à définir avec la direction artistique.
