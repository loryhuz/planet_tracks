# Cibles de ressenti : voiture Rally de TMUF sur terre (mesures sur le vrai jeu)

Ce document ne contient que du comportement observé, comme `feel-targets.md` pour la SnowCar. Les données sont des états par tick du vrai TrackMania United Forever (TMInterface, 100 Hz, 1 tick = 10 ms) avec la voiture Rally, enregistrés en injectant des entrées scriptées. On n'a lu aucun code du jeu et on ne cite aucune constante interne : chaque nombre sort des fichiers `data/tmuf/rally/*.csv` du dépôt Trackmania-Original. On peut tout régénérer avec `rally_analyze.py` (qui produit `rally-dirt.json`), puis `rally_plots.py` (qui produit `plots/rally_*.svg`).

**Unités** : km/h ; m/s² (« g » = 9,81 m/s², pour comparer avec la neige) ; rad/s ; degrés. Distances en mètres du jeu, |v|·3,6 = compteur.
**Angle de dérive** : angle entre le cap de la caisse et la vitesse horizontale, **positif quand le nez pointe plus loin dans le virage que la trajectoire** (queue sortie). **Vitesse latérale** : composante de la vitesse sur l'axe transversal de la caisse (glissement de la caisse sur le côté).
**Roues** : w0 avant gauche, w1 avant droite, w2 arrière droite, w3 arrière gauche (comme la SnowCar).

**Matériaux** (colonne `wNmat`), identifiés par les enregistrements et la géométrie des blocs :

- 17 : **piste en terre** (route Rally, gravier). Couloir de 8 m de large, 4 m sous le terrain, bordé de talus d'herbe.
- 6 : **champ** (terre / chaume, blocs « Field »). Surface plane sur tout le bloc, au niveau de l'herbe : c'est la seule grande surface de terre.
- 19 : **pavé** (routes « château » et « medium »). C'est l'équivalent Rally de l'asphalte.
- 20 : herbe (terrain, talus, bords de champ).
- 14 : bois ; 12 : pierre (murs, rochers) ; 7 : turbo ; 2 : sol hors carte.

**Piste en terre (17) et champ (6) donnent les mêmes nombres, à quelques % près,** en ligne droite, au freinage et sur les petits coups de volant. Les manœuvres larges ont donc été faites sur le champ.

---

## Cartes et contrôles

- **RallyB1** (campagne Nadeo) : départ sur la piste en terre. On l'a aussi utilisée pour les runs auteur, rejoués sur A1–A5 et B1–B5.
- **Cartes de test générées sans l'éditeur** (`tools/gbx/rally_testmap.py`, commande `GbxTool mapgen`). Chacune garde les blocs de RallyB1 et remplit sa moitié ouest, qui est vide :
  - `RDirtT5` : ligne droite de piste en terre de 26 blocs (832 m, parfaitement plate).
  - `RDirtLab` : départ, rampe de sortie, puis un **champ de 544 × 576 m** (17 × 18 blocs), avec de l'herbe au-delà.
  - `RPavedLab` : départ « medium » et quadrillage de carrefours pavés. Le pavé est plat, mais chaque croisement porte un îlot d'herbe surélevé de 6 m de rayon. Les manœuvres larges sur pavé touchent donc un îlot, et on ne garde que la partie d'avant ce contact (voir « Limites »).
- **Entrées vérifiées** sur chaque enregistrement : gas, brake et steer suivent le script sur 100 % des ticks (99,8 % pour un saut). Cause probable des enregistrements « gas = 0 » de la session LPC : **le jeu ignore toute entrée injectée tant que sa fenêtre n'est pas au premier plan** (reproduit ici : 0 % d'entrées appliquées en arrière-plan, 100 % au premier plan). `rally.py` la remet donc au premier plan avant chaque essai.
- **Déterminisme** : `r_field_step100R` rejoué deux fois donne toutes les colonnes physiques identiques au bit près. Seule la caméra diffère.
- **Symétrie** : gauche et droite sont des miroirs exacts (échelons à 100 et 140 km/h).
- **Gravité en vol** : 40,0 m/s², comme la SnowCar.

---

## 1. Ligne droite (terre, champ, pavé)

Voir `plots/rally_accel_surfaces.svg`.

- **Accélération plein gaz, en paliers constants**, identiques sur piste en terre, champ et pavé partout où plusieurs surfaces ont été mesurées. De 0 à 75 km/h, les paliers ne viennent que de la piste en terre, et de 75 à 100 km/h que du champ. Chaque changement se fait en 1 tick.

  | Plage (km/h) | m/s² |
  |---|---|
  | 0 – 25 | 60 |
  | 25 – 50 | 35 |
  | 50 – 75 | 25 |
  | 75 – 100 | 20 |
  | 100 – 125 | 8 |
  | 125 – 200 | 6 |
  | 200 – 327 | 4 (toujours 4 à 327 km/h, en fin de ligne droite) |

- **Temps** (piste en terre, départ arrêté) :

  | Vitesse | 60 | 100 | 140 | 200 | 250 | 300 km/h |
  |---|---|---|---|---|---|---|
  | Temps | 0,44 s | 0,92 s | 2,42 s | 5,2 s | 8,7 s | 12,1 s |

  Vitesse max : non atteinte. 327 km/h au bout des 832 m, avec encore +4 m/s².
- **Roue libre** : −1,0 m/s² sur les trois surfaces, de 60 à 200 km/h.
- **Frein** : pendant tout le freinage, 96 à 99 % des roues sont marquées en glisse.

  | Surface | Décélération (> 35 km/h) | Sous 35 km/h | 100 → 0 | 140 → 0 | 200 → 0 |
  |---|---|---|---|---|---|
  | Terre (piste et champ) | −51 m/s² | −85 à −110 m/s² | 0,45 s, 7,3 m | 0,67 s, 14,6 m | 1,0 s, 30 m |
  | Pavé | −43,5 m/s² | — | 0,53 s, 8,6 m | 0,78 s, 17,1 m | — |

- **Aucun tangage** : les amortisseurs avant et arrière restent égaux sous −51 m/s² comme sous +25 m/s².

## 2. Virage en adhérence (régime établi)

Voir `plots/rally_radius_vs_speed.svg`. Vitesse tenue par l'accélérateur, 7 s par essai.

- **Braquage max sur le champ** :

  | km/h | Rayon (m) | Lacet (rad/s) | Dérive | a latérale | Roulis |
  |---|---|---|---|---|---|
  | 41 | 7,8 | 1,47 | −1,1° | 1,7 g | 1,1° |
  | 61 | 8,3 | 2,02 | −0,2° | 3,5 g | 2,3° |
  | 80 | 9,3 | 2,41 | 0,4° | 5,5 g | 4,0° |
  | **96 (plafond)** | 9,4 | **2,84** | 2,3° | **7,75 g (76 m/s²)** | 5,1° |

  **Plafond** : en braquage max, la voiture ne dépasse pas 96,3 km/h, même plein gaz (consignes 100 et 120 km/h). Les roues avant sont alors marquées en glisse, les arrière non.
- **Lacet proportionnel au volant** (analogique, champ, 100 km/h) :

  | Volant | 25 % | 50 % | 75 % | 100 % |
  |---|---|---|---|---|
  | Lacet (rad/s) | 0,65 | 1,30 | 1,95 | 2,85 |
  | Rayon (m) | 43 | 21,5 | 14,3 | 9,5 |
  | Dérive | 0,2° | 0,4° | 0,6° | 2,4° |

  Demi-braquage à 140 km/h : R = 29,7 m, 5,2 g, dérive 0,5°. C'est encore de l'adhérence.
- **Pavé et herbe, en adhérence** : mêmes rayons que le champ. À 41 km/h, R = 7,8 m ; demi-braquage à 100 km/h, R = 21,3 m. À 60 km/h, pavé et herbe donnent R = 7,6 m (lacet 2,21) avec les roues avant marquées en glisse, contre 8,3 m sur le champ.
- **Lacet** : en adhérence, il suit v/R et monte donc avec la vitesse (1,47 rad/s à 41 km/h, 2,41 à 80, 2,84 à 96). Il plafonne ensuite à **2,85 – 2,88 rad/s** : au-delà de 96 km/h, il ne dépasse plus cette valeur, quelle que soit la vitesse, y compris en glisse.

## 3. Échelon de braquage (braquage max 1,5 s puis relâché)

Voir `plots/rally_drift_steps.svg`. Vitesse tenue jusqu'au coup de volant, puis plein gaz (« gaz ») ou pied levé (« levé ») à partir du coup de volant.

| Surface | Départ | Pic de dérive | Instant du pic | t63 | Vitesse mini | Perte | v latérale max | Retour (< 2°) après relâché |
|---|---|---|---|---|---|---|---|---|
| champ, gaz | 60 | 2,3° (établi) | — | 0,41 s | accélère à 95 | 0 | 1,1 m/s | 0,26 s |
| champ, gaz | 80 | 2,3° (établi) | — | 0,26 s | accélère à 96 | 0 | 1,1 m/s | 0,26 s |
| champ, gaz | 100 | **11,4°** | 0,78 s | 0,47 s | 75 | −25 km/h | 4,8 m/s | 0,25 s |
| champ, gaz | 120 | **24,6°** | 0,82 s | 0,50 s | 61 | −59 km/h | 10,0 m/s | 0,25 s |
| champ, gaz | 140 | **36,5°** | 0,88 s | 0,54 s | 51 | −89 km/h | 14,5 m/s | 0,24 s |
| champ, levé | 60 | 0° | — | — | 48 | −14 km/h | 0 | — |
| champ, levé | 100 | 7,0° | 0,64 s | 0,35 s | 71 | −29 km/h | 3,0 m/s | 0,25 s |
| champ, levé | 140 | 36,4° | 0,89 s | 0,55 s | 35 | −105 km/h | 14,2 m/s | 0,19 s |
| herbe, gaz | 100 | **66,9°** | 0,99 s | 0,61 s | 33 | −67 km/h | 17,7 m/s | 0,20 s |
| pavé, gaz | 100 | 14,1° à 0,5 s (champ : 7,9°) | contact îlot à 0,69 s | | | | | |
| pavé, gaz | 140 | ≈ 49 – 52° | 0,86 s | 0,58 s | 56 (à 1,25 s) | −84 km/h | | contact îlot à 1,3 s |
| pavé, levé | 140 | 57,9° | 1,0 s | 0,63 s | 11 | −129 km/h | 17,9 m/s | |

- **Construction de l'angle** : le lacet atteint 2,6 rad/s en 0,25 s et plafonne à 2,85. La trajectoire, elle, ne tourne qu'au rythme permis par l'adhérence :

  | Départ (champ, gaz) | Lacet à 0,5 s | Trajectoire à 0,5 s | Dérive à 0,5 s |
  |---|---|---|---|
  | 100 km/h | 2,85 rad/s | 2,48 rad/s | 7,9° |
  | 140 km/h | 2,87 rad/s | 1,84 rad/s | 21° |

  L'angle grandit donc de (lacet − trajectoire) : environ 60°/s à 140 km/h, 20°/s à 100 km/h.
- **Fin de glisse** : la perte de vitesse fait remonter le rythme de trajectoire permis (a_lat / v). Vers **50 – 60 km/h plein gaz** (40 – 55 km/h pied levé), la trajectoire rattrape la caisse. La dérive retombe alors de 33° à 1° en 0,25 s, pendant que le volant est encore braqué, puis la voiture tourne en adhérence (R ≈ 9 m, dérive 2°).
- **Trajectoire** : le cap et la course (direction de la vitesse) changent d'autant sur l'essai, par exemple 205° et 204° à 140 km/h. La voiture finit donc le virage, simplement plus large et plus lente. Pendant la glisse, le rayon vaut 14 à 25 m ; en adhérence, 9 m.
- **Relâché** : le lacet est multiplié par **0,89 par tick** (τ ≈ 0,09 s) et la dérive disparaît en **0,25 s**, sans contre-oscillation (< 1,5°).

## 4. Perte de vitesse en glisse

Voir `plots/rally_speed_loss.svg`. Le taux de perte est à peu près **proportionnel à l'angle de dérive**.

| Dérive | 5° | 10° | 20° | 30° | 36° | 58° |
|---|---|---|---|---|---|---|
| Champ, sans gaz (m/s²) | −6 | −14 | −25 | −36 | −42 | — |
| Champ, plein gaz (m/s²) | −5,5 | −11,5 | −23 | −33 | −36 | — |
| Pavé, sans gaz (m/s²) | −6 | −12 | −21 | −30 | −36 | −51 |
| Herbe, plein gaz (m/s²) | — | — | — | — | −15 | −17 |

- **Champ** : environ −1,1 m/s² par degré. Le gaz ne compense que quelques m/s².
- **Pavé** : environ −0,85 m/s² par degré.
- **Herbe** : la voiture glisse beaucoup plus loin en perdant moins par degré ; c'est la surface la plus proche de la glace.
- **En adhérence** (dérive ≤ 2,5°), le plein gaz accélère encore (+8 m/s²).

## 5. Coups de volant (impulsions)

Au clavier, à vitesse tenue.

| Durée | Lacet max | Instant | Cap | Dérive max à 100 km/h | Dérive max à 140 km/h | Perte à 140 km/h |
|---|---|---|---|---|---|---|
| 50 ms | 1,02 rad/s | 0,08 s | 7,4° | 0,6° | 0,6° | 0,1 km/h |
| 100 ms | 1,67 rad/s | 0,12 s | 15° | 1,0° | 1,0° | 0,5 km/h |
| 200 ms | 2,39 rad/s | 0,21 s | 31° | 1,6° | 5,8° | 1,7 km/h |
| 300 ms | 2,71 rad/s | 0,31 s | 48° | 2,5° | 11,2° | 8,9 km/h |
| 500 ms | 2,84 rad/s | 0,51 s | 80° | 7,0° | 22,7° | 32 km/h |

- Retour du lacet sous 0,05 rad/s : 0,35 s (50 ms) à 0,9 s (500 ms). Jusqu'à 300 ms, la course suit le cap au dixième de degré près.
- Les mêmes coups sur la piste en terre (17) et sur le pavé donnent les mêmes nombres à ±5 % pour 50 à 100 ms. Au-delà, la voiture sort du couloir de 8 m, ou touche un îlot sur le pavé.

## 6. Contre-braquage, lever de pied, freinage en appui

Voir `plots/rally_countersteer_lift.svg`. Essais sur le champ, à 100 km/h.

- **Contre-braquage** : 1 s à droite (dérive 8,4°, lacet 2,65 rad/s, 77 km/h), puis gauche à fond.
  - Le lacet s'inverse en 0,11 s et la dérive revient à 0 en 0,15 s.
  - La voiture repart dans l'autre sens avec 2,5 – 3° de dérive, sans balancier ni tête-à-queue.
  - C'est identique avec un contre-braquage de 0,5 s ou de 1 s.
- **Lever de pied en appui** (braquage max, 96 km/h) :
  - rien ne tourne : la dérive passe de 2,3 à 2,1° et le rayon de 9,4 à 8,8 m ;
  - la décélération est de −4 m/s².
- **Freiner en appui** :
  - freinage à −54 m/s², dérive toujours ≤ 2,8° ;
  - arrêt en 0,43 s, sans tête-à-queue ;
  - aucun effet de transfert de charge (voir aussi « aucun tangage »).

## 7. Transitions

- **Champ → herbe en virage** (demi-braquage, 140 km/h) :
  - avec deux roues extérieures sur l'herbe pendant 0,55 s, la dérive passe seulement de 0,5 à 0,8° ;
  - avec les quatre roues sur l'herbe, le même demi-braquage monte à **40,6° de dérive en 1,8 s** et la vitesse tombe à 58 km/h (0,5° sur le champ).
- **Piste en terre → pavé en ligne droite** (run auteur A2, 170 km/h) : aucune variation visible.
- **Piste en terre ↔ champ** : comportements identiques, la transition ne se voit pas.

## 8. Saut

Rampe de sortie de RDirtLab, voir `rally-dirt.json` → `jump`.

- Décollage à 120 km/h avec +5 à +6 m/s de vitesse verticale et 9 – 10° de nez levé.
- 0,27 – 0,29 s en l'air pour 9 – 10 m parcourus, sous une gravité de 40,0 m/s².
- À l'atterrissage (−5,6 m/s), les 4 amortisseurs vont en butée (0,01), puis reviennent au repos en 0,5 s. La vitesse n'est pas affectée.

## 9. Suspension et roulis

- **Amortisseurs** (`wNdamper`) :
  - 0,25 à l'apparition, qui tombe en 0,3 s vers 0,03 puis se stabilise à **0,062 en roulant**, sur toutes les surfaces ;
  - butée : 0,006 – 0,013.
  - Les valeurs n'ont pas la même échelle que la SnowCar (0,25 au repos).
- **Roulis** : environ 0,7° par g, côté extérieur comprimé.

  | a latérale | 1,7 g | 3,5 g | 5,5 g | 7,75 g |
  |---|---|---|---|---|
  | Roulis | 1,1° | 2,3° | 4,0° | 5,1° |
  | Amortisseurs extérieur / intérieur | 0,046 / 0,077 | | | 0,013 (butée) / 0,15 |

- **Tangage** : aucun au freinage ni à l'accélération.

## 10. Conduite réelle (runs auteur Nadeo)

Ticks où les 4 roues sont sur la piste en terre (17), au-dessus de 30 km/h.

Chaque run est utilisé jusqu'à son premier choc : rejoués, la plupart se désynchronisent. Seuls A2, A4, B3 et B5 vont jusqu'à l'arrivée.

| Run (jusqu'à) | Vitesse médiane / max (km/h) | Dérive p50 / p95 / max | Volant tourné | a latérale p95 |
|---|---|---|---|---|
| B1 (10,1 s) | 134 / 167 | 0,1° / 1,5° / 7,5° | 18 % du temps | 6,7 g |
| B4 (5,9 s) | 128 / 171 | 0,0° / 0,9° / 3,1° | 11 % du temps | 6,4 g |
| A2 (arrivée) | 122 / 170 | 0,1° / 0,7° / 11,5° | 16 % du temps | 3,4 g |
| A5 (8,6 s) | 141 / 177 | 0,0° / 0,6° / 9,4° | 4 % du temps | 3,8 g |

Les pilotes Nadeo tapent le volant et restent presque toujours en adhérence : des dérives de 3 à 12° ne durent qu'un instant, sans glisse longue.

---

## Cibles clés terre

| # | Cible | Valeur mesurée | Source |
|---|---|---|---|
| 1 | Accélération plein gaz (toutes surfaces) | 60 / 35 / 25 / 20 / 8 / 6 / 4 m/s², seuils 25 / 50 / 75 / 100 / 125 / 200 km/h, paliers francs | `r_t5_accel`, `r_field_accel`, `r_paved_accel` |
| 2 | 0→100 / 0→200 km/h | 0,92 s / 5,2 s ; > 327 km/h en 832 m | `r_t5_accel` |
| 3 | Roue libre | −1,0 m/s² (terre, champ, pavé) | `r_*_coast*` |
| 4 | Frein | terre −51 m/s² (100→0 : 7,3 m, 0,45 s) ; pavé −43,5 m/s² (8,6 m) | `r_*_brake*` |
| 5 | Lacet max | 2,85 – 2,88 rad/s, atteint vers 96 km/h en braquage max puis plafonné quelle que soit la vitesse (en adhérence : v/R) | tous les virages |
| 6 | Rayon mini (braquage max) | 7,8 m à 40, 8,3 m à 60, 9,4 m à 96 km/h ; rayon ∝ 1/volant (43 / 21,5 / 14,3 / 9,5 m à 25 / 50 / 75 / 100 %, 100 km/h) | `r_field_circle*`, `r_field_analog100` |
| 7 | Limite d'adhérence (champ) | 7,75 g (76 m/s²) en régime établi ; braquage max plafonné à 96 km/h, dérive 2,3° | `r_field_circle100` / `120` |
| 8 | Latéral en glisse | champ ≈ 5,5 – 6,7 g ; pavé ≈ 3,2 – 5,6 g (≈ 4,5 g à 100 km/h) ; herbe ≈ 3,3 g | balayages `*_circle_coast`, `r_grass_step100R` |
| 9 | Dérive d'un échelon (champ, gaz) | 2° à 60–80, 11° à 100, 25° à 120, 36° à 140 km/h ; pic à 0,8 – 0,9 s, t63 0,47 – 0,54 s | `r_field_step*` |
| 10 | Dérive d'un échelon (autres surfaces) | pavé ≈ 1,4 – 1,8× le champ (140 : 50 – 58°) ; herbe à 100 km/h : 67° | `r_paved_step*`, `r_grass_step100R` |
| 11 | Vitesse de construction de l'angle | lacet − trajectoire : ≈ 60°/s à 140, 20°/s à 100 km/h | `r_field_step*` |
| 12 | Perte de vitesse en glisse | ≈ −1,1 m/s² par degré (champ) ; échelon 140 → 51 km/h en 1,2 s | `decel_vs_drift` |
| 13 | Retour d'adhérence | la dérive s'effondre (33° → 1° en 0,25 s) vers 50 – 60 km/h plein gaz, volant toujours braqué | `r_field_step140*` |
| 14 | Relâché / contre-braquage | lacet ×0,89 par tick, dérive nulle en 0,25 s ; contre-braquage : lacet inversé en 0,11 s, sans balancier | steps, `r_field_flick100` |
| 15 | Transfert de charge | aucun : pas de survirage au lever de pied ni au freinage, pas de tangage | `r_field_liftoff100`, `braketurn100` |
| 16 | Roulis / suspension | 0,7°/g (5° à 7,75 g) ; amortisseurs 0,062 en roulant, butée 0,01 ; atterrissage en butée puis repos en 0,5 s | cercles, saut |
| 17 | Air | g = 40 m/s² | saut |

## Comment la Rally dérive sur terre, en mots simples

**Tant que le volant ne demande pas plus que l'adhérence** (en braquage max : sous ≈ 95 km/h ; en demi-braquage : jusqu'à 140 km/h au moins), la voiture Rally tourne « sur des rails ». La caisse et la trajectoire tournent ensemble et le nez n'est décalé que de 1 à 2,5°. **Quand on braque trop fort pour la vitesse**, la caisse continue de pivoter à son rythme maximal (≈ 2,85 rad/s, le même qu'à basse vitesse), alors que la trajectoire ne s'incurve qu'autant que l'adhérence le permet. L'arrière sort donc progressivement, en 0,5 à 0,9 s, jusqu'à 11° à 100 km/h et 36° à 140 km/h. La voiture ne part pas pour autant en translation : la trajectoire reste franchement courbée (rayon de 14 à 25 m au lieu de 9 m, 5 à 6 g de latéral), et le virage se termine dans la direction voulue, simplement plus large. Cette glisse coûte cher en vitesse, environ 1 m/s² par degré d'angle (140 → 51 km/h en 1,2 s), et c'est justement cette perte qui la termine : dès que la vitesse redescend assez pour que l'adhérence suffise, l'angle s'effondre en un quart de seconde et la voiture repart en adhérence, volant toujours braqué. Il n'y a ni dérive entretenue, ni effet de poids (lever le pied ou freiner ne fait pas pivoter), ni balancier au contre-braquage : relâcher ou contre-braquer efface l'angle en 0,15 à 0,25 s. Sur le pavé, la même manœuvre glisse 1,4 à 1,8 fois plus. Sur l'herbe, elle glisse beaucoup plus (67° contre 11° à 100 km/h) en perdant moins de vitesse par degré : c'est la seule vraie « glace » du Rally.

## Limites et incertitudes

- **Pavé** : le quadrillage pavé a des îlots d'herbe tous les 32 m. Les échelons à 60 et 100 km/h et les cercles au-delà de 60 km/h n'y sont exploitables que jusqu'au premier contact (0,7 à 1,3 s), et seul l'échelon « levé » à 140 km/h est propre de bout en bout. La limite d'adhérence établie sur pavé n'est donc pas mesurée : on a seulement « plus glissant que le champ » en transitoire. La ligne droite, les cercles à 40 km/h, les demi-braquages et les coups de volant courts sont propres.
- **Piste en terre (17)** : elle est trop étroite pour les manœuvres larges. Elle est identique au champ sur tout ce qui a pu être comparé (accélération, frein, roue libre, coups de 50 à 100 ms), mais les grandes glisses n'ont été mesurées que sur le champ (6).
- **Deux régimes à 60 km/h** : pavé et herbe donnent R = 7,6 m avec les roues avant en glisse, le champ R = 8,3 m sans glisse. La cause n'est pas identifiée.
- **Consigne de vitesse** : les cercles « à vitesse tenue » utilisent un gaz en tout-ou-rien (±0,5 km/h), et non un gaz partiel.
- **Direction analogique** : seul un balayage 25 / 50 / 75 / 100 % à 100 km/h a été enregistré.
- **Non mesuré** : la vitesse max (encore +4 m/s² à 327 km/h), la marche arrière, les murs sur terre, et un atterrissage de grand saut (le seul saut dure 0,28 s).
