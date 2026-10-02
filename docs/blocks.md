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
- Une pente d'un niveau sur une cellule fait 14° en moyenne (26° au plus raide), sur deux
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

Le catalogue de l'éditeur va jusqu'à 3 cellules et 2 niveaux pour les virages et pentes, et de 4
à 8 cellules pour les réceptions.

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
