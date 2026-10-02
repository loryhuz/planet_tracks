# Décors de camp : direction artistique des décors

Validé le 2 octobre 2026 (moodboards ci-dessous), fusée et mine comprises.

## L'idée

Autour des circuits, la colonie vit. Elle a construit ses camps comme ses routes, avec ce qu'elle a
sur place (`art/roads/brief.md`) : bâche blanche laminée, tissu gonflable cerclé de bandes orange,
tubes de plastique rouge, sangles orange, sacs de régolithe, piquets d'acier, conteneurs du fret,
panneaux solaires. Un camp et pas une ville : tout est monté à la main, tendu, haubané, un peu de
travers, poussiéreux.

Les décors donnent au circuit son échelle et sa profondeur : de petites choses au bord de la piste,
des camps à quelques centaines de mètres, et d'énormes silhouettes au loin, sur les mesas, devant
la tempête.

## Le catalogue

Du plus petit au plus grand (le buggy fait 4 m) :

| Décor | Taille | Ce que c'est |
|---|---|---|
| dépôt | 4 à 8 m | caisses et fûts sanglés sous un appentis de bâche, manche à air, jalons à fanion |
| poste d'observation | 12 à 20 m | tour en treillis de tubes rouges sur piles de sacs, haubanée ; cabine à bandeau vitré, antennes, parabole, gyrophare |
| camp de terrain | 30 à 50 m | deux à quatre tentes-dômes, un conteneur, panneaux solaires, mâts à fanions |
| camp de base | 100 à 150 m | dômes et modules-tunnels gonflables avec sas ronds, citernes sur berceaux rouges, conteneurs, champ solaire, murets de sacs, rover |
| mât de communication | 60 à 100 m | treillis rouge haubané, paraboles, balise rouge au sommet |
| zone d'atterrissage | 80 à 120 m | aire en dalles blanches cerclée de sacs, fusée posée (50 m), citernes ; une mine à côté (excavatrice, convoyeur sur treillis, tas de minerai) |
| colonie | 300 à 500 m | dômes géants tenus par des filets de sangles, serres vitrées, tour de 150 à 200 m, fusée |

## Le placement

- **Près de la piste** (20 à 80 m) : dépôts, postes d'observation sur les éperons qui dominent un
  virage, camps de terrain le long de la terre. Ils encadrent la route comme les reliefs.
- **À mi-distance** (100 à 400 m) : camps de base, mâts, zones d'atterrissage.
- **Au loin** (500 à 1 100 m, avant la tempête) : la colonie, les dômes géants et les grandes
  tours, sur les mesas, en silhouettes adoucies par la poussière.
- Sur chaque carte : une grande silhouette visible depuis la plupart des sections, un camp près du
  départ, quelques postes sur les hauteurs.

## La construction

- Modélisé par le code, comme les routes et les portiques, avec les mêmes pièces (tubes, bâche,
  sacs, sangles) ; jamais par un générateur 3D à base d'IA.
- Ce qui est posé à la main varie : tailles, angles, teintes, espacements. Jamais le même objet
  répété à intervalle fixe.
- Vu de loin, peu de grandes formes lisibles (dômes, tour, fusée), pas une dentelle qui scintille.
- Le sol est aplani sous les camps, comme sous les routes.
- Près de la piste, les décors sont solides (la voiture les heurte) ; au loin, ils ne sont que
  dessinés.
- Matières en plus de celles des routes : tissu des dômes, vitrages, panneaux solaires, conteneurs
  peints, balises lumineuses.

## Moodboards

Générés avec Higgsfield (GPT Image 2.5), dans `moodboard/` :

1. `01-planche.jpg` : la planche du kit, du dépôt à la colonie, avec le buggy à l'échelle.
2. `02-colonie-au-loin.jpg` : la colonie sur sa mesa vue de la caméra de course, un poste au bord
   de la route.
3. `03-camp-de-base.jpg` : un camp de base au bord de la piste.
4. `04-poste-observation.jpg` : le poste d'observation sur son rocher.
5. `05-petits-depots.jpg` : les petits décors le long de la terre.
6. `06-dome-geant.jpg` : un dôme de 300 m et une tour de 200 m vus du pied.
7. `07-crepuscule.jpg` : un camp et la colonie au crépuscule.
8. `08-zone-atterrissage.jpg` : la zone d'atterrissage et la mine.

## Dans le jeu

`crates/track/src/camp.rs` construit les décors depuis la liste `structures` d'une carte
(`{"structure":"post","position":[x,z],"yaw":degrés}`, ou `structure post x z yaw` dans un fichier
`.chain`). Premiers décors, sur Jezero :

- `post` : le poste d'observation, sur la petite butte qui domine les premiers virages ;
- `base_camp` : le camp de base, dans la première boucle, face au départ ;
- `colony` : la colonie, sur la longue mesa au nord (élargie à 82 m de rayon pour elle), vue du
  départ, de la ligne droite surélevée et de la terre.

Le terrain est aplani sous les camps (le relief des buttes et des mesas est gardé). Ce qui est à
portée de la voiture est solide ; la colonie n'est que dessinée. Les dômes proches montrent le
grain de leur toile, les lointains un enduit blanc lisse. Captures : `in-game/`.

À faire : la zone d'atterrissage avec sa mine, le dépôt, le camp de terrain, le mât de
communication, puis les autres cartes.
