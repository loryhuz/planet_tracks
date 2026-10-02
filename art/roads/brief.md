# Routes de camp : direction artistique des routes

Validé le 2 octobre 2026 (moodboards ci-dessous).

## Le constat

Les blocs route actuels (bitume gris, vibreurs rouge et blanc, flancs pleins sous les sections
surélevées) font circuit terrestre posé sur Mars. Surélevés, ce sont des dalles massives sans
raison d'être : rien n'explique comment on aurait coulé du bitume et du béton ici.

## L'idée

Sur Mars, les colons construisent avec ce qu'ils ont sur place : du plastique (imprimé, recyclé
des emballages du fret, ou fabriqué localement), des sacs remplis de régolithe, des sangles, des
bâches. Les routes sont du matériel de camping spatial à l'échelle d'un circuit : légères, montées
vite, réparées sur place, tendues plutôt que coulées.

## Les pièces

### Les pilotis (sections surélevées)

- Tubes en plastique rouge vif, de plusieurs diamètres, assemblés en treillis croisé (croix de
  Saint-André, triangles), comme un échafaudage.
- Raccords en T et en croix, colliers de serrage, sangles à cliquet orange, quelques platines.
- Pieds sur des sacs de régolithe empilés ou des platines vissées au sol ; haubans tendus jusqu'à
  des piquets.
- Le dessous est ajouré : on voit le ciel et le sol à travers la structure. C'est ce qui rend une
  section surélevée belle, là où le bloc plein actuel est le plus laid.

### Le tablier (la chaussée)

- Une membrane plastifiée tendue sur des panneaux rigides : bâche tressée enduite, ou panneaux
  gonflables haute pression (type « drop-stitch » des paddles), plats et durs.
- Couleur claire (blanc cassé, gris clair) qui se détache du sol et des tubes, avec des bandes et
  des flèches orange et noires au pochoir, dans l'esprit de la livrée du buggy.
- Détails : soudures entre les lés, œillets, rustines d'une autre couleur, numéros de panneau
  peints, poussière orange dans les plis et les coins.
- Rives : boudins gonflables rouge et blanc à la place des vibreurs, filets ou sangles tendus entre
  des poteaux à la place des glissières.

### Au sol

- La même bâche plaquée sur le régolithe, tenue par de grandes sardines et des sacs de sable le
  long des bords.
- Vers la terre, la bâche disparaît sous la poussière et laisse place à des grilles plastiques
  alvéolées, puis à la piste creusée (transition progressive, jamais une coupure nette).

### L'habillage

- Portiques de départ et d'arrivée en tubes et toile tendue, comme une arche de tente.
- Autour du circuit, un camp et pas une ville : tentes et dômes gonflables, conteneurs, panneaux
  solaires, mâts à fanions, manches à air.
- La nuit, bandes LED le long des rives et projecteurs sous les treillis.

## Contraintes de jeu

- La chaussée reste lisse et lisible : le grain de la bâche, les coutures et les œillets passent
  par la texture et la normal map, jamais par la géométrie de la surface roulante ni par la
  physique.
- Le rouge des tubes ne doit pas se fondre dans le sol martien : un rouge plus saturé et plus froid
  que le régolithe, éventuellement alterné avec du blanc.
- Le treillis reste simple vu de loin (quelques gros tubes, pas une dentelle qui scintille à
  distance).
- Matériel technique propre mais usé, comme le buggy : blanc, orange, noir, rouge.

## Moodboards

Générés avec Higgsfield (GPT Image 2.5), dans `moodboard/` :

1. `01-planche.jpg` : planche de matières et de couleurs.
2. `02-pilotis-canyon.jpg` : route sur pilotis au-dessus d'un canyon, vue d'ensemble.
3. `03-sous-le-tablier.jpg` : le treillis vu de dessous.
4. `04-chaussee.jpg` : étude de la surface roulante et des rives.
5. `05-au-sol.jpg` : la route posée au sol et sa transition vers la terre.
6. `06-virage-releve.jpg` : virage relevé sur treillis.
7. `07-depart-camp.jpg` : la ligne de départ dans le camp.
8. `08-crepuscule.jpg` : la route au crépuscule, avec ses LED.

## Dans le jeu

Mis en œuvre le 2 octobre 2026. Les blocs, leurs variantes et leur construction sont décrits dans
`docs/blocks.md` ; les captures sont dans `in-game/`.

- **La chaussée** est la bâche `art/textures/src/tarp.png`, avec ses lés soudés, ses rustines et
  ses marquages au pochoir.
- **Deux variantes de bord**, au choix pour chaque bloc :
  - A, `sandbags` : une rangée de sacs de sable, et des sardines qui plantent la bâche ;
  - B, `bumpers` : des boudins rouge et blanc, et la bâche sanglée.
  
  Par défaut, un bloc qui quitte le sol prend les boudins, un bloc au sol les sacs.
- **Les routes surélevées** reposent sur des poutres en treillis rouges. Les piles irrégulières
  sur grosses piles de sacs ne descendent au sol que tous les 24 m environ, les appuis bas sont
  en sacs, et des sangles orange retiennent la dalle au sol.
- **Les textures** : `tarp`, `sandbag` (toile kaki) et `webbing` (sangle orange).

- **Les portiques** de départ, de checkpoint et d'arrivée : une arche de gros tubes rouges, des
  manchons blancs, une banderole à damier « PLANET TRACKS », des sacs et des sangles au pied ; la
  ligne et le mot (« DÉPART », « CHECKPOINT », « ARRIVÉE ») peints sur la bâche.

Reste à faire : le camp autour du départ.
