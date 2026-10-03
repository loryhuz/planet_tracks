# Planète de glace : le kit de blocs

Le kit de blocs de la planète de glace, tenu à part de `docs/blocks.md` (Mars) pour ne pas
mélanger les deux. Il est publié dans l'onglet « Glace » de la page partagée :
https://claude.ai/artifact/QNgJoQ6GnMenV8nMVZ1USZ

## Les matières

Sur la planète de glace (`planet ice`), les surfaces du kit sont d'autres matières, et la voiture
à skis (`physics::neige`) les conduit à sa façon :

| Bloc | Devient | Pour la voiture |
|---|---|---|
| tablier de route | glace vive, bleu glaçon (une texture à elle, sans la bâche de Mars : ni panneaux, ni soudures, ni marquages) | peu d'adhérence, glisse jusqu'à 45°, la trajectoire suit le nez avec retard |
| `dirt` | neige tassée, 28 m | les roues s'enfoncent de 10 cm, le moteur tire environ moitié moins que sur la glace (≈ 225 km/h au plus), glisses courtes et chères, traces |
| `snow` | la même neige, **16 m** | pareil, sur une piste étroite : la neige est plus lente que la glace, ses parties sont serrées |
| sol | poudreuse | les roues s'enfoncent de 22 cm, la voiture laboure : freinée avec le carré de sa vitesse vers ≈ 110 km/h, vite quand elle sort à pleine vitesse |

La glace est la même partout : tablier, parois et lèvres des gouttières, flancs des dalles
surélevées (`ice` dans `tools/textures/bake.py`, d'une photo générée). Les bords (sacs, boudins),
les pilotis et les portiques sont encore ceux de Mars.

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

## Les gouttières de bobsleigh

La variante **`gutter`** fait d'une route de glace une gouttière de bobsleigh, un couloir de glace
en U où l'on choisit sa hauteur sur la paroi. Elle se pose sur tous les blocs de route, les
portiques compris (lignes droites, virages, S, pentes, virages qui montent ou descendent), pas
sur les passages à la neige.

| Mesure | Valeur |
|---|---|
| Fond | plat sur 10 m (5 m de chaque côté du milieu) |
| Parois | un quart d'ellipse depuis le bord du fond : 9 m vers l'extérieur, 6 m de haut là où elle devient verticale. Elle monte doucement (rayon de 13,5 m au pied, moins de 30° jusqu'à 11 m du milieu, comme le tablier de 20 m d'une route) puis se raidit (4 m de rayon en haut) |
| Paroi conduite | jusqu'à 70° de pente, à 4,6 m de haut |
| Haut de paroi | au-dessus de la verticale, une lèvre de 1,4 m de rayon s'enroule au-dessus de la piste |
| Sommet | 7,8 m au-dessus du fond ; la gouttière tient dans une cellule (32 m) |
| Portiques | leurs poteaux sur le haut des parois, l'arche au-dessus (8,8 m), le déclencheur large de toute la paroi : la gouttière continue sous eux |
| Extrémités | au bout d'une suite de gouttières, les parois naissent de rien sur 24 m et le fond s'élargit en tablier de route : une gouttière se raccorde à n'importe quelle route |

La première version (un fond de 20 m et des parois en quart de cercle de 5 m de rayon) se
conduisait comme des murs : en montant dessus à l'extérieur d'un virage, la voiture prenait des
chocs de 150 à 400 m/s², décollait de la paroi et retombait. Le pied en pente douce de l'ellipse
et les réglages ci-dessous en font une trajectoire.

Pour la voiture à skis (et elle seule : celle de Mars n'en sait rien) :

- **elle reste collée à la paroi** : sur une pente pavée de plus de 19 à 24° (les virages relevés
  de Sulcus penchent de 18° au plus) jusqu'à 55 à 70°, une force la tient contre la paroi avec
  le carré de sa vitesse (4 g à 200 km/h), sans quoi la détente de sa suspension la décolle de la
  paroi qu'elle longe ; pas près du haut, d'où une voiture montée trop vite doit retomber ;
- sur une paroi de plus de 30° (pleinement à 45°), la gravité tire entière, au lieu de 45 % dans
  les pentes : monter coûte de la vitesse, la voiture redescend, et sa vitesse dit à quelle
  hauteur elle roule ;
- la vitesse qui monte la paroi est absorbée (14 par seconde) : une voiture qui arrive de face y
  monte et en redescend au lieu d'être jetée par-dessus ; celle qui longe la paroi garde sa
  vitesse ;
- jetée en l'air par une paroi, elle se remet à l'endroit (et ne retombe pas sur le toit).

Ce que ça donne (`cargo run -p physics --release --example gutter_lines`, et le test
`crates/physics/tests/gutter_map.rs`) : dans des virages progressifs à 250 km/h, une ligne à 40
à 50° sur la paroi extérieure est **plus rapide que le milieu** (10,9 s contre 11,3 s : la paroi
tourne la voiture, qui sinon drifterait et perdrait de la vitesse), sans choc (33 m/s² au plus)
ni décollage. Sans toucher au volant, une voiture traverse toutes sortes de virages, `turn1`
compris, en restant dedans.

**Règles de construction** : des virages à la mesure de la vitesse. Les `curve3`, `curve4` et
`sbend3`, `sbend4` se prennent à fond et sur la paroi ; les `curve2`, `sbend2` et `turnN`
seulement là où la voiture est lente (au départ, après une montée) : à 250 km/h, la paroi d'un
`curve2` jette la voiture par-dessus. Les virages s'enchaînent sans ligne droite, d'une paroi à
l'autre ; les portiques restent dans la gouttière.

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

## La piste de bobsleigh : Canalis

`crates/track/maps/canalis.chain` (version 2), série facile, ≈ 1,7 km, 28,8 s au pilote d'essai
sur le milieu, 27,5 s sur les parois extérieures (or 27,6 s). Toute en gouttières, portiques
compris, elle descend une montagne par paliers, de 56 m à la plaine. La première version
enchaînait des `curve2` et des S serrés à plus de 200 km/h : on s'y prenait les murs.
Celle-ci est faite pour les parois :

- **le haut**, lent : un `curve2_right_down1` serré tout de suite au départ, qui plonge dans la
  gouttière, puis un `sbend3` ;
- **la première moitié** : deux longs `curve4` (le premier descend d'un niveau), un `sbend3`, un
  `curve3_right_down1`, puis le premier point de passage ;
- **la seconde moitié** : un `curve3_left_down1`, une pente, un `curve3`, un `sbend3` et deux
  `curve3_right_down1` à la suite, puis le second point de passage dans la plaine ;
- **la plaine** : un `sbend3` jusqu'à l'arrivée.

Pas de ligne droite entre les virages : la voiture bascule d'une paroi à l'autre. Une mesa sous
chaque palier de la piste, chacune s'étendant au-delà de l'endroit où la piste la quitte, pour
que la gouttière descende dans une tranchée plutôt que du bord d'une falaise ; quelques buttes.
Pas encore de décor propre à la planète.

## Planche des formes

Dessinée depuis la géométrie du jeu par `cargo run -p track --release --example shapes`, dans
`docs/blocks-ice/` : les virages progressifs (à plat et relevés), les S, les serpentins, le
passage à la neige, la chicane et l'épingle en neige, des virages qui montent et descendent, et
des gouttières (une ligne droite, un virage progressif, un virage qui descend).

## À faire

- Un jeu de décors propre à cette planète, sans les camps ni la colonie de Mars.
- Des bords, des pilotis et des portiques à elle, à définir avec la direction artistique.
