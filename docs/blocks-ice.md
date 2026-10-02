# Planète de glace : brief du kit de blocs

Brief du kit de blocs et des décors de la planète de glace, tenu à part de `docs/blocks.md`
(Mars) pour ne pas mélanger les deux kits. Il est publié dans l'onglet « Glace » de la page
partagée : https://claude.ai/artifact/QNgJoQ6GnMenV8nMVZ1USZ

## Où on en est

- Un prototype jouable, **Noctis Neige** (`crates/track/maps/noctis_neige.chain`) : le tracé de
  Noctis avec `planet ice`, conduit par la voiture à skis (`physics::neige`).
- Il réutilise les blocs de Mars tels quels. Seules les matières changent : un tablier de route
  devient de la **glace vive**, la terre de la **neige tassée**, le sol de la **poudreuse**. Elles
  sont en couleurs unies, sans texture.
- Les bords (sacs, boudins), les pilotis, les portiques et les décors sont encore ceux de Mars.

## Ce qu'on a appris en jouant (2 octobre 2026)

- **La glace se joue comme une voiture de drift** : on est en glisse tout le long et on gère
  l'angle. Les quarts de virage secs du kit de Mars (rayon 16 m) cassent ce rythme.
- **La neige est lente et lourde** : la voiture plafonne vers 165 km/h sur le plat et chaque
  glisse coûte cher en vitesse. Les couloirs de terre de Mars (28 m de large, plus à l'extérieur
  des virages) y sont **trop larges** : on s'y ennuie.

Les mesures qui guident les tailles :

| | Glace | Neige | Mars, pour comparer |
|---|---|---|---|
| Rayon à fond de braquage, 120 km/h | 16 m | 10,6 m | 12 m (route), 13 m (terre) |
| Rayon à fond de braquage, 200 km/h | 22 m | 13 m | 19 m (route et terre) |
| Vitesse au plus, sur le plat | 300 km/h | ≈ 165 km/h | 300 km/h |
| Angle de glisse | jusqu'à 45°, monte en ≈ 1 s | 8 à 12° | 9° en terre |

## À faire : les blocs de glace

- **Des virages plus arrondis, faits pour drifter** : de longues courbes plutôt que des quarts
  de tour secs, des rayons qui se resserrent ou s'ouvrent progressivement, et des
  enchaînements gauche-droite qui font basculer la glisse d'un côté à l'autre.
- **Jamais de virage serré à plat sur la glace.** La glace ne tourne serré que si la piste la fait
  tourner : virages relevés, gouttières de bobsleigh où l'on choisit sa hauteur sur la paroi.
  Ailleurs, des lignes droites et de grandes courbes.
- La glace reste la partie rapide du circuit.

## À faire : les parties en neige

- **Plus serrées et plus sinueuses**, puisqu'on y va lentement : un couloir plus étroit que les
  28 m de la terre de Mars, des virages courts enchaînés, des épingles, des chicanes.
- La neige reste le cœur de la planète : c'est là qu'on conduit le plus.

## À faire : les décors et les bords

- Un jeu de décors propre à cette planète, sans les camps ni la colonie de Mars.
- Des bords, des pilotis et des portiques à elle (à définir avec la direction artistique).
