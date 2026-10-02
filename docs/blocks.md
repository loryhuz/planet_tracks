# Les blocs de Planet Tracks

Référence des blocs avec lesquels on construit un circuit, de leurs variantes et de la façon dont
le jeu les construit. Ce fichier est tenu à jour avec le code : la source de vérité reste
`crates/track/src/map.rs` (catalogue, variantes), `crates/track/src/kit.rs` (géométrie) et
`crates/track/src/stilts.rs` (pilotis). Direction artistique : `art/roads/brief.md`.

Version partagée (page privée, republiée depuis ce fichier à chaque mise à jour) :
https://claude.ai/artifact/QNgJoQ6GnMenV8nMVZ1USZ

## La grille

- Une **cellule** fait 32 m × 32 m, un **niveau** 8 m de haut.
- La **route** fait 20 m de large ; la **terre**, 28 m (plus à l'extérieur des virages).
- Les blocs se raccordent au milieu des bords de cellule, à niveau entier, à plat.
- Un bloc est orienté : on y entre par `cell` (sa cellule d'entrée), au niveau `level`, avec la
  direction `rotation` (quarts de tour vers la gauche depuis le nord : 0 = +Z, 1 = +X, 2 = −Z,
  3 = −X).
- Une pente d'un niveau sur une cellule fait 14° en moyenne (27° au plus raide), sur deux
  cellules 7°, sur trois 5°.

## Le catalogue

`N` = nombre de cellules, `L` = nombre de niveaux.

| Bloc | Ce que c'est |
|---|---|
| `start`, `checkpoint`, `finish` | une cellule droite avec son portique de départ, de passage ou d'arrivée (voir « Les portiques ») |
| `straight`, `straightN` | ligne droite d'une ou `N` cellules |
| `turnN_left`, `turnN_right` | quart de virage sur `N × N` cellules (rayons 16, 48, 80 m pour N = 1, 2, 3) |
| `bankedN_left/right` | quart de virage relevé de 18° autour de son axe (pour les plateformes) |
| `bermN_left/right` | virage au sol, l'extérieur relevé de 18° (moins en terre sur les virages courts) |
| `ubermN_left/right` | demi-tour au sol (`2N × N` cellules), l'extérieur relevé |
| `slopeN_upL`, `slopeN_downL` | montée ou descente de `L` niveaux sur `N` cellules |
| `whoopsN` | `N` cellules de bosses, une bosse de 0,6 m par cellule |
| `to_dirt`, `to_road` | une cellule dont la deuxième moitié passe de la route à la terre (ou l'inverse) |
| `jump_ramp` | une cellule qui monte vers une lèvre à 4°, suivie d'un vide |
| `landingN_downL` | la réception d'un `jump_ramp` : posée sur la cellule après la rampe, au niveau de la rampe, elle finit `L` niveaux plus bas |
| `kicker` | une cellule qui monte vers une lèvre à 20° : un grand saut |
| `kicker_landingN_downL` | la colline de réception d'un `kicker`, posée comme `landingN_downL` |
| `…_left`, `…_right` sur une réception | la même réception qui décale d'une cellule sur le côté (un S sous le vol) |

Le catalogue de l'éditeur va jusqu'à 3 cellules pour les virages, 4 cellules et 2 niveaux pour
les pentes, et de 4 à 8 cellules pour les réceptions. Le code accepte des tailles plus grandes
(`straight7`, `slope6_up3`…, jusqu'à 32 cellules et 8 niveaux), toujours sur la grille.

## La planche des formes

Un bloc, c'est une **forme** (ci-dessous) et une **variante** (bâche bordée de sacs ou de
boudins, ou terre : « Les variantes »). Chaque variante s'applique à toutes les formes.

### L'élévation

- La hauteur se compte en **niveaux de 8 m**. Chaque bloc commence et finit **à plat, à un
  niveau entier**, au milieu d'un bord de cellule.
- On ne change de niveau qu'avec une pente (`slopeN_upL`, `slopeN_downL`) ou la réception d'un
  saut. L'angle n'est pas libre : il découle de `N` et `L`. Deux pentes à la suite repassent par
  le plat entre elles.
- Le dévers des virages relevés vaut 18° (moins sur la terre dans les petits virages). Il se met
  en place et disparaît à l'intérieur du bloc.
- Il n'y a donc ni virage qui monte, ni pente ou dévers qui continue d'un bloc à l'autre, ni angle
  hors de la grille.

### Comment lire les vignettes

Les vignettes sont dessinées depuis la géométrie du jeu par
`cargo run -p track --release --example shapes` (`crates/track/examples/shapes.rs`), à relancer
quand un bloc change. On y voit :

- la grille de 32 m, avec en plus foncé les cellules qu'occupe le bloc ;
- la chaussée : de la bâche, qui se teinte d'abricot à mesure qu'elle monte, ou de la terre. Des
  traits en travers tous les 8 m font lire le dévers et la pente. Une flèche indique le sens ;
- les bords de la variante automatique : sacs de sable (beige) au sol, boudins (rouge) en
  hauteur ;
- sous ce qui quitte le sol, un rideau rose jusqu'au sol, marqué d'un trait rouge tous les 8 m,
  et la hauteur des raccords en mètres (pour un virage relevé au sol, celle de son extérieur au
  milieu du virage) ;
- pour un saut, la trajectoire d'une voiture qui se pose au milieu de la réception.

Seules les versions `_left` sont dessinées : `_right` en est le miroir. Une descente
`slopeN_downL` est la montée `slopeN_upL` prise dans l'autre sens. Les vignettes d'une ligne sont
à la même échelle, sauf qu'un bloc plus petit y est agrandi (deux fois au plus).

### Lignes droites

| `straight` | `straight2` | `straight3` |
|---|---|---|
| ![straight](blocks/straight.svg) | ![straight2](blocks/straight2.svg) | ![straight3](blocks/straight3.svg) |

### Portiques

| `start` | `checkpoint` | `finish` |
|---|---|---|
| ![start](blocks/start.svg) | ![checkpoint](blocks/checkpoint.svg) | ![finish](blocks/finish.svg) |

### Virages plats

| `turn1_left` | `turn2_left` | `turn3_left` |
|---|---|---|
| ![turn1_left](blocks/turn1_left.svg) | ![turn2_left](blocks/turn2_left.svg) | ![turn3_left](blocks/turn3_left.svg) |

### Virages relevés en hauteur

Relevés autour de leur axe : l'intérieur descend autant que l'extérieur monte, d'où leur place sur
une route surélevée (ici au niveau 1).

| `banked1_left` | `banked2_left` | `banked3_left` |
|---|---|---|
| ![banked1_left](blocks/banked1_left.svg) | ![banked2_left](blocks/banked2_left.svg) | ![banked3_left](blocks/banked3_left.svg) |

### Virages relevés au sol

L'intérieur reste au sol, l'extérieur monte.

| `berm1_left` | `berm2_left` | `berm3_left` |
|---|---|---|
| ![berm1_left](blocks/berm1_left.svg) | ![berm2_left](blocks/berm2_left.svg) | ![berm3_left](blocks/berm3_left.svg) |

### Demi-tours relevés au sol

| `uberm1_left` | `uberm2_left` | `uberm3_left` |
|---|---|---|
| ![uberm1_left](blocks/uberm1_left.svg) | ![uberm2_left](blocks/uberm2_left.svg) | ![uberm3_left](blocks/uberm3_left.svg) |

### Montées d'un niveau

| `slope1_up1` | `slope2_up1` | `slope3_up1` | `slope4_up1` |
|---|---|---|---|
| ![slope1_up1](blocks/slope1_up1.svg) | ![slope2_up1](blocks/slope2_up1.svg) | ![slope3_up1](blocks/slope3_up1.svg) | ![slope4_up1](blocks/slope4_up1.svg) |

### Montées de deux niveaux

| `slope1_up2` | `slope2_up2` | `slope3_up2` | `slope4_up2` |
|---|---|---|---|
| ![slope1_up2](blocks/slope1_up2.svg) | ![slope2_up2](blocks/slope2_up2.svg) | ![slope3_up2](blocks/slope3_up2.svg) | ![slope4_up2](blocks/slope4_up2.svg) |

### Bosses

| `whoops1` | `whoops2` | `whoops3` |
|---|---|---|
| ![whoops1](blocks/whoops1.svg) | ![whoops2](blocks/whoops2.svg) | ![whoops3](blocks/whoops3.svg) |

### Terre

Sur Mars, la terre est creusée dans le terrain, plus large à l'extérieur des virages et aux bords
irréguliers : les vignettes en montrent la forme de base.

| `to_dirt` | `to_road` | `berm2_left dirt` |
|---|---|---|
| ![to_dirt](blocks/to_dirt.svg) | ![to_road](blocks/to_road.svg) | ![berm2_left dirt](blocks/berm2_left_dirt.svg) |

### Sauts

La rampe et sa réception, posées l'une après l'autre. Les réceptions existent de 4 à 8 cellules,
sur 1 ou 2 niveaux, droites ou décalées d'une cellule à gauche ou à droite.

| `jump_ramp` + `landing4_down1` | `kicker` + `kicker_landing5_down2` | `kicker` + `kicker_landing5_down2_left` |
|---|---|---|
| ![jump_ramp + landing4_down1](blocks/landing4_down1.svg) | ![kicker + kicker_landing5_down2](blocks/kicker_landing5_down2.svg) | ![kicker + kicker_landing5_down2_left](blocks/kicker_landing5_down2_left.svg) |

## Les variantes

Chaque bloc prend une variante facultative (`"variant"` dans le JSON, deuxième mot de la ligne
dans un fichier `.chain`).

| Variante | Chaussée | Bords |
|---|---|---|
| *(aucune)* ou `road` | bâche | **automatique** : boudins si le bloc quitte le sol (niveau d'entrée ou de sortie au-dessus de 0, rampe, réception), sacs de sable s'il reste au sol |
| `sandbags` | bâche, plantée de sardines | **A** : une rangée de sacs de sable de chaque côté |
| `bumpers` | bâche, sanglée | **B** : des boudins rouge et blanc de chaque côté |
| `dirt` | terre | aucun : un couloir creusé dans le sol |

- `to_dirt` et `to_road` n'acceptent pas de variante : leur moitié route prend les bords
  automatiques. Ces bords s'effacent sur les 6 derniers mètres avant la terre et repoussent après.
- Les portiques (`start`, `checkpoint`, `finish`) acceptent les variantes comme une ligne droite.

Exemples :

```
{"block":"berm3_right","cell":[-12,4],"level":0,"rotation":1,"variant":"bumpers"}
```

```
berm3_right bumpers
straight2 sandbags
turn2_left dirt
```

Olympus utilise les deux variantes : départ en sacs de sable, fin technique en boudins
(`crates/track/maps/olympus.chain`, version 4).

## Comment le jeu construit une route

### Au sol

- La chaussée est une bâche blanc cassé tendue. On y voit des lés de 5 m soudés tous les 12 m,
  quelques panneaux d'une autre teinte, des rustines, une ligne orange et des tirets noirs au
  pochoir le long des bords, la gomme sur les trajectoires et de la poussière martienne.
- Entre la chaussée et le bord, une bande de 0,5 m (l'accotement d'origine) où la bâche continue
  jusqu'au bord, où elle s'arrête. Une voiture qui frôle le bord ne touche donc pas les sacs ou les boudins tant que
  ses roues restent sur la route.
- **Variante A** (d'après le moodboard 05), de la route vers l'extérieur : la bâche, dont le bord
  extrême est planté de piquets, puis les sacs, posés sur le sol martien hors de la bâche, collés
  à elle (leur ventre la recouvre de 5 cm au plus).
  - Les piquets : à peu près un par sac (quelques-uns manquent), une barre d'acier hexagonale
    d'environ 9 cm qui dépasse de 35 à 55 cm, sous un chapeau rond avec une poignée pour
    l'arracher ; chacun penché de 3 à 16° dans sa propre direction et tourné à sa façon, la
    plupart galvanisés, quelques-uns rouillés. Des œillets percés à la main courent le long du
    bord de la bâche.
  - Les sacs : de gros sacs de toile remplis, posés à plat bout à bout, jamais empilés, chacun
    différent : 1,4 à 1,9 m de long, 0,86 à 1 m de large, 0,44 à 0,54 m de haut (certains plus
    remplis que d'autres), bombés et rentrés dessous, bosselés, un peu de travers, de teinte et de
    poussière propres.
- **Variante B** : un boudin rond de 0,6 m de diamètre, rouge et blanc (longueurs de 2 m), sanglé
  d'orange au milieu de chaque longueur blanche, tous les 4 m. La bâche est sanglée : œillets tous
  les 0,9 m, et une sangle avec sa boucle à cliquet jusqu'au boudin, tous les 4 m.
- Au-delà du bord, un accotement descend vers le terrain.

### En hauteur

- Plus la route monte, plus la bande de terre se referme (elle a disparu à 1,2 m de haut). Le bord
  se colle alors au tablier : c'est la barrière qu'ont toujours eue les routes surélevées.
- La route devient une dalle de 0,8 m d'épaisseur, fermée dessous par de la bâche. Elle quitte le
  sol entre 1 et 2,5 m de haut.
- **Pilotis** (rouge vif, en tubes de plastique) :
  - à partir de 2,6 m sous la dalle, une poutre en treillis court sous chaque côté du tablier
    (1,8 m de haut, panneaux de 4 m), reliée à l'autre par des traverses ;
  - des **piles** descendent jusqu'au sol, en moyenne tous les 24 m, à 8 m près, irrégulièrement.
    Chaque pile a deux poteaux croisillonnés sous chaque poutre, posés sur un tas de gros sacs
    de sable croisés, chacun de travers et de teinte propre, et les deux jambes sont
    contreventées entre elles ;
  - plus près du sol, la dalle repose tous les 8 à 24 m sur des piles de sacs, avec un poteau
    dessus quand l'espace dépasse 1,3 m ;
  - des **sangles orange** partent des bords de la dalle vers des sardines plantées dans le sol,
    avec leur cliquet.
- Là où la roche remonte sous la route, aucun pilotis.

### En terre

Un couloir creusé dans le terrain, large, relevé dans les virages, aux bords irréguliers.

## Ce qui compte pour la conduite

- La surface de roulage n'a pas changé : mêmes temps aux pilotes automatiques sur les quatre
  cartes (Jezero 35,53 s, Olympus 28,70 s, Noctis 29,25 s, Ares Vallis 27,47 s).
- **Ce qu'on voit des bords n'est pas ce que la voiture touche.** Pour elle, un bord est une boîte
  invisible à face droite côté piste : 0,5 × 0,5 m sous les boudins (exactement l'ancienne lèvre),
  0,6 × 0,44 m sous les sacs. Une arête arrondie est une marche que la roue escalade, et la
  voiture passait alors par-dessus. Le tube rond et les sacs arrondis ne sont que dessinés, de
  même que les sacs des piles, posés sur une boîte invisible.
- **Au sol, on ne sort plus de la piste** : une voiture qui quitte la route tape dans les sacs ou
  les boudins, au lieu de rouler sur l'accotement comme avant.
- Les tubes des pilotis sont solides. Les sangles, sardines et boucles ne sont que dessinées. Une
  voiture tombée d'une route surélevée peut maintenant passer dessous.

## Les portiques

Le départ, les checkpoints et l'arrivée ont le même portique, construit comme les routes
(moodboard 07, `crates/track/src/gates.rs`) :

- une arche de gros tubes de plastique rouge (28 cm de rayon), deux côte à côte dans chaque
  jambe et le long du haut, cintrés aux angles, tenus par des colliers gris ;
- des manchons de toile blanche sur le haut des jambes ;
- sous le haut, une banderole de toile blanche de 1,8 m : un damier à chaque bout, « PLANET
  TRACKS » au milieu au pochoir (Saira Stencil One, la police du logo), un trait orange dessous,
  lisible des deux côtés ;
- quatre gros sacs de sable autour du pied de chaque jambe, et deux sangles orange par jambe vers
  des piquets plantés dans le sol.

Ce qui distingue les portiques est peint sur la bâche, à lire par le pilote qui arrive :

- départ : une ligne à damier, et « DÉPART » en grandes lettres au pochoir (15 × 3 m) 3 m avant ;
- checkpoint : une ligne orange, et « CHECKPOINT » ;
- arrivée : une ligne à damier, et « ARRIVÉE ».

Sur la terre, il n'y a ni ligne ni mot : seul le portique, plus large, se dresse sur les talus.

Pour la voiture, rien ne change : elle rencontre les poteaux et la poutre qu'avaient les anciens
portiques, en boîtes invisibles. L'arche, la banderole et les accessoires ne sont que dessinés.

## Les décors

Autour du circuit, la colonie : ses camps sont faits comme les routes (bâche, tissu gonflable
cerclé de sangles orange, tubes rouges, sacs de régolithe), plus ce que le fret a apporté
(citernes et conteneurs peints, panneaux solaires, vitres, balises, une fusée). Direction
artistique : `art/scenery/brief.md` ; code : `crates/track/src/camp.rs`.

Une carte les place dans sa liste `structures` :

```
{"structure":"base_camp","position":[402.0,-172.0],"yaw":165.0}
```

```
structure base_camp 402 -172 165
```

| Décor | Taille | Ce que c'est |
|---|---|---|
| `post` | 13 à 17 m | poste d'observation : tour en treillis de tubes rouges sur piles de sacs, haubanée de sangles, cabine à bandeau vitré, antennes, parabole, gyrophare |
| `base_camp` | 85 × 70 m | camp de base, sa façade vers `yaw` : dômes et tunnels gonflables avec sas, citernes sur berceaux, conteneurs, champ solaire, murets de sacs, mât en treillis, fanions, manche à air, rover |
| `colony` | 460 m de long, le long de `yaw` | la colonie, faite pour être vue de loin : dôme géant de 92 m tenu par un filet de sangles, deux dômes, serres vitrées, citernes, conteneurs, champs solaires, tour de 170 m à balises, fusée sur son aire |

- Le terrain est aplani sous les camps ; les buttes et les mesas gardent leur relief.
- Ce qui est à portée de la voiture (postes, camps) est solide ; la colonie n'est que dessinée.
- Tout ce qui est posé à la main varie avec le décor (tailles, angles, écarts).
- Sur Jezero : le poste sur la petite butte des premiers virages, le camp de base dans la première
  boucle face au départ, la colonie sur la longue mesa au nord. Captures : `art/scenery/in-game/`.

## Matières

Textures cuites par `tools/textures/bake.py` à partir de `art/textures/src/` :

- `tarp` (couche 0) : la bâche des chaussées et des dalles ;
- `sandbag` (couche 8) : la toile des sacs ;
- `webbing` (couche 9) : la sangle orange ;
- `signs` (couche 12) : les inscriptions des portiques, dessinées par `tools/textures/signs.py`
  (« PLANET TRACKS », « DÉPART », « ARRIVÉE », « CHECKPOINT ») ;
- `galvanized` (couche 10) et `rust` (couche 11) : l'acier galvanisé des piquets et des boucles,
  avec ses cristaux de zinc, et l'acier rouillé de quelques piquets. L'acier est rendu comme du
  métal nu : il reflète le ciel au-dessus de l'horizon et le sol plus sombre en dessous, chaque
  face de la barre hexagonale montrant une partie différente, avec un éclat net du soleil.

Les tubes sont en plastique brillant, avec leurs colliers gris peints aux extrémités. Le shader de
`crates/app/src/shaders/scene.wgsl` dessine les lés, les marquages, les sacs, les sangles, les
œillets et les sardines.

## Captures

`art/roads/in-game/` : vue aérienne et dessous du tablier de Jezero, dessous du kicker de
Noctis, route au sol en sacs (A, au ras du sol, de près et vue d'en haut) et en boudins (B), route
surélevée vue du tablier ; les portiques (départ, checkpoint, arrivée, sur la terre, de dos).
