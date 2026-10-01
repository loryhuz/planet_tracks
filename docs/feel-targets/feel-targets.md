# Cibles de ressenti : SnowCar de TMUF (mesures sur le vrai jeu)

Ce document ne contient que du comportement observé. Les données sont des états par tick du vrai TrackMania United Forever (TMInterface, 100 Hz, 1 tick = 10 ms), enregistrés sur la carte SnowB1. On n'a lu aucun code du jeu et on ne cite aucune constante interne : chaque nombre sort des fichiers `data/tmuf/*.csv`. On peut tout régénérer avec `analyze.py` (qui produit `feel-targets.json`), puis `plots.py` (qui produit `plots/*.svg`).

**Unités** : km/h ; m/s² (1 g = 9,81 m/s²) ; rad/s ; degrés. Les distances sont en « mètres » du jeu, avec |v|·3,6 égal au compteur.
**Matériaux** (colonne `wNmat`) :

- 14 : route (ligne droite de départ, pont, circuit) ;
- 21 : terrain enneigé (zone ouverte au sud du départ, collines) ;
- 12 : petite surface non identifiée sur un flanc de colline, où la voiture reste bloquée en marche arrière ;
- 0 : pas de contact.

**Ordre des roues** (déduit du roulis et des atterrissages) : w0 avant gauche, w1 avant droite, w2 arrière droite, w3 arrière gauche.

**Enregistrements inutilisables** :

- `lpc/*.csv` : les entrées n'ont jamais été appliquées (gas = 0), la voiture reste immobile au départ.
- `nadeo_run1.csv` : replay désynchronisé, la voiture tape la barrière à 2,15 s puis reste arrêtée. On ne l'utilise que comme exemple de choc.
- `nadeo`, `nadeo_finish` et `nadeo_cam_*` sont un même run déterministe, identique au bit près.

---

## 1. Ligne droite (route plate)

**Accélération plein gaz, en paliers constants** (voir `plots/accel_vs_speed.svg`). Chaque saut est instantané, sur 1 tick. Les paliers ne dépendent pas du rapport engagé et on ne voit aucune traînée proportionnelle à la vitesse.

| Plage (km/h) | m/s² | g |
|---|---|---|
| 0 – 55 | 30,0 | 3,06 |
| 55 – 80 | 17,85 | 1,82 |
| 80 – 115 | 9,0 | 0,92 |
| 115 – 140 | 5,0 | 0,51 |
| 140 – 240 | 2,5 | 0,25 |
| > 240 | 1,0 | 0,10 (vu de 240 à 242,3 seulement) |

Sources : `accel`, `taps`, `steer_*`, `x_*` (ligne droite de départ), `nadeo_finish` et `x_cp_post` (portions plates à 140–160 et 230–242 km/h). Entre 160 et 230 km/h, le palier à 2,5 est confirmé par les rampes du pont après correction de la pente (2,51).

- **Pente** : plein gaz, l'accélération change de −30,0 m/s² par unité de sinus de pente (p10–p90 : −30,4 / −29,7). C'est 75 % de la gravité mesurée en l'air (40 m/s²). Sources : rampes du pont dans `nadeo_finish`, `x_cp_post` et `x_crest_off`.
- **Temps** (voir `plots/speed_vs_time.svg`) :
  - 0→100 km/h : 1,53 s et 26,6 m, mesuré dans `accel`.
  - 0→200 : ≈ 10,0 s et 404 m ; 0→240 : ≈ 14,5 s et 676 m. Ces deux valeurs viennent des paliers mesurés, intégrés sur route plate.
  - Run réel `nadeo_finish` : 100 km/h à 1,56 s et 200 km/h à 10,25 s (avec virages et pont).
  - 0→300 : environ 31 s *si* le palier à 1 m/s² continue. C'est une **extrapolation** : aucune mesure au-delà de 242 km/h.
- **Vitesse max** : non atteinte. Le maximum observé est 242,3 km/h, et la voiture accélère encore de +1,0 m/s² (`nadeo_finish`, t = 15,8 s).
- **Rapports** : passages 1→2 à 70 km/h, 2→3 à 119, 3→4 à 172, 4→5 à 235. Ils sont purement cosmétiques, car l'accélération ne change pas au passage. La colonne `rpm` vaut toujours 0 (non capturée).
- **Départ** : la voiture apparaît 0,3 m en l'air et se pose pendant les 0,25 premières secondes (accélération transitoire de 21 à 44 m/s²). Ensuite, 30 m/s² exactement.
- **Roue libre** : −1,0 m/s², à 88–95 km/h sur route (`coast`) comme à 28–31 km/h sur neige (`x_snow_*`). C'est constant, sans dépendance à la vitesse visible. Non mesuré au-dessus de 95 km/h.
  `coast.csv` contient un artefact : 1 tick de frein au relâchement (t = 1,41 s, −1,8 km/h). La valeur ci-dessus est mesurée après ce tick.
- **Frein (route)** : −51 m/s² (5,2 g) constant de 117 à 34 km/h, puis −80 m/s² sous environ 33 km/h, où le frein se fond dans la marche arrière.

  | Départ | Temps d'arrêt | Distance |
  |---|---|---|
  | 96 km/h (`brake`) | 0,46 s | 6,8 m |
  | 117 km/h (`x_wall_brake_rev`) | 0,58 s | 10,2 m |
  | 93 km/h (`x_reverse_steer`) | 0,44 s | 6,4 m |

  Garder le frein après l'arrêt passe en marche arrière. Voir `plots/brake_coast_reverse.svg`.
- **Marche arrière** :
  - Route : 30 m/s² (comme le 1er palier), avec un **plafond dur à 150,2 km/h** (cycle 150,0–150,4) atteint en 1,4 s (`x_wall_brake_rev`).
  - Neige : la vitesse converge vers **30 km/h**, avec −10 m/s² au-dessus de ce seuil (`reverse`, `x_reverse_hill`).
- **Aucun tangage** : les 4 amortisseurs restent égaux et le tangage reste à 0°, sous +30 m/s² d'accélération comme sous −51 m/s² de freinage.

## 2. Direction (route)

Voir `plots/steer_response.svg`, `plots/turning_vs_speed.svg` et `plots/slip_vs_speed.svg`.

- **Entrée → roues** : 1 tick de retard, puis une rampe linéaire de 0,2 par tick. Le braquage max est atteint en **50 ms**, et le retour à 0 se fait avec la même rampe (227 événements clavier).
- **Lacet (échelon, braquage max)** :
  - accélération angulaire initiale : 16,5–17 rad/s² ;
  - 63 % du lacet final en **0,15 s**, 90 % en 0,28–0,32 s (vérifié à 90, 118 et 241 km/h, `steer_left_nogas`, `x_crest_off`, `nadeo_finish`).
  - Au relâchement, le lacet est multiplié par **0,92 par tick** (τ ≈ 0,12 s).
- **Braquage max en régime établi** (voir le tableau ci-dessous). Sous 80 km/h, le rayon est à peu près constant, **≈ 9,5–10 m** (`steer_left`, 57–81 km/h). Au-dessus, le lacet plafonne vers 2,4–2,7 rad/s et le rayon croît à peu près linéairement avec la vitesse. L'adhérence n'est jamais saturée : l'accélération latérale monte à 17 g avec moins de 2° de glisse.

  | km/h | Lacet (rad/s) | Rayon (m) | a latérale (g) | Glisse (°) | Perte d'accél. vs ligne droite (m/s²) | Source |
  |---|---|---|---|---|---|---|
  | 78 (sans gaz) | 2,35 | 9,1 | 5,2 | 0,9 | −4,7 | `steer_left_nogas` |
  | 84 | 2,44 | 9,6 | 5,8 | 0,7 | −5,0 | `x_corner_late`, `steer_left` |
  | 109 | 2,54 | 12,0 | 7,8 | 0,1 | −5,3 | `x_wall_right` |
  | 121 | 2,55 | 13,3 | 8,7 | 0,1 | −5,6 | `x_crest_off` |
  | 228 | 2,66 | 23,7 | 17,2 | 1,0 | −7,1 | `nadeo_finish` (épingle finale : 147° en 1 s) |

- **Glisse (angle vitesse/cap)**, sur les ticks propres (4 roues au sol, plus de 20 km/h, hors choc) :
  - p95 de 2,8° à 40–60 km/h, ≤ 1,4° à 60–80 et ≤ 1,2° au-dessus de 80 km/h ;
  - maximum 4,3° ;
  - drapeau de glisse par roue : **0 %** sur route.
  - Le drapeau `sliding` au niveau de la voiture vaut toujours 0, il est inutilisable.
- **Perte de vitesse en virage** : environ −5 m/s² à 80–120 km/h et −7 m/s² à 230 km/h, par rapport à la ligne droite, avec ou sans gaz.
  - Avec gaz à 84 km/h : +4,0 au lieu de +9,0.
  - Sans gaz à 80 km/h : −5,8 au lieu de −1,0.
  - À 228 km/h : −4,5 au lieu de +2,5.

## 3. Hors-piste : neige (matériau 21)

Sources : `x_snow_donuts` et `x_snow_uturn_hill`, sur terrain plat.

- **Traction** : environ 0,5–0,6 fois la route. Le palier des 55 km/h existe aussi sur neige.

  | km/h | Braquage | m/s² | Rapport vs route (même vitesse) |
  |---|---|---|---|
  | 10 – 50 | max | 15,0 – 16,2 | 0,50 – 0,54 |
  | 50 – 60 | max | 8,8 | — |
  | 60 – 70 | tout droit | 10,7 | 0,60 |
  | 80 – 100 | tout droit | 5,6 – 5,7 | 0,63 |

- **Roue libre** : −1,0 m/s², comme sur route.
- **Freinage** : gaz appliqué en roulant en arrière, de 21 à 0 km/h : **−21 m/s²**, soit 0,41 fois la route. Aucun freinage en marche avant sur neige n'a été enregistré.
- **Marche arrière** : plafond à 30 km/h (150 sur route).
- **Braquage max, 45–83 km/h** :
  - accélération latérale : **2,46 g** (p50), 2,82 g (p90) ;
  - la caisse tourne presque comme sur route (2,09 rad/s), mais la trajectoire ne tourne qu'à **1,29 rad/s** ;
  - la glisse monte de 15–20° par 0,1 s jusqu'à environ 80°, ce qui mène au tête-à-queue (402° de cap en 4 s). Il n'y a pas de dérive stable.
  - Les 4 roues sont en glisse 78 % du temps.
- **Basse vitesse (16–53 km/h), braquage max** : rayon ≈ **10,8 m**, avec les roues avant seules en glisse (sous-virage).
- **Glisse sur neige** : p50 de 1 à 19°, p95 de 32 à 83° selon la vitesse. Au moins une roue glisse 84 % du temps.
- **Vitesse** : 100 km/h au plus sur neige, limité par la zone de test, pas par la voiture.

## 4. Murs (barrières en bois de SnowB1)

L'impact est **inélastique**. La composante normale est annulée sans rebond, avec un fort frottement tangentiel. Au-delà d'environ 50°, la voiture monte sur la barrière ou bascule.

| Événement | Avant (km/h) | Incidence | Après 50 ms | Conservé | Déviation | Suite |
|---|---|---|---|---|---|---|
| `x_wall_head`, frontal (barrière chanfreinée) | 131,9 | ~75–80° | 32,5 | 25 % | 37° | monte sur la barrière (up.y 0,46), bloquée |
| `coast`, frontal en roue libre | 87,6 | ~80° | 8,6 | 10 % | 24° | arrêt |
| `steer_left`, perpendiculaire | 82,3 | 88° | 7,1 | 9 % | 7° | arrêt |
| `nadeo_run1` / `x_corner_late` 4,14 s | 110 / 101 | 87° | 6,5 / 4,6 | 5–6 % | — | arrêt |
| `x_wall_right`, latéral | 110,3 | 57° (géométrie connue) | 22,4 | 20 % | 37° | sur le flanc (up.y 0,32), 80 % de la vitesse retrouvés en 4,6 s |
| `x_cp_post`, 2e choc (barrière) | 159,1 | ~76° | 24,1 | 15 % | 47° | — |
| `x_corner_late`, entrée tardive | 129,7 | ~48° | 83,0 | 64 % | 24° | continue |
| `x_cp_post`, poteau de checkpoint | 219,9 | ~42° | 154,5 | 70 % | 23° | continue, réaccélère normalement |
| `x_wall_right`, frottement (×3) | 81 – 85 | 10 – 16° | 71 – 76 | 86 – 89 % | 5 – 6° | 1–2 % perdus par tick de contact |

Rebond mesuré (vitesse s'éloignant du mur dans les 0,3 s) : au plus 0,7 m/s sur les chocs à plus de 55°, et 2 à 11 m/s sur les chocs obliques. L'incidence est estimée à partir de la direction de l'impulsion (±10–20°).

## 5. En l'air

Voir `plots/jumps.svg`.

- **Gravité effective : 40,0 m/s² (4,08 g)**. 89 % des ticks en vol sont exactement à −40,0 (303 ticks mesurés).
- **Aucune traînée** : la vitesse horizontale est strictement constante en vol, et le gaz n'a aucun effet.
- **Vols mesurés** :

  | Vol | Décollage | Durée | Distance | Chute | Détail |
  |---|---|---|---|---|---|
  | `x_crest_off` | 123 km/h, vy −2,8 m/s | 1,08 s | 36,5 m | 26,1 m | contact à vy −46 m/s |
  | `x_ramp_off_left` | 119 km/h | 0,60 s | 19,8 m | 7,2 m | — |
  | `x_ramp_off_right` | — | 0,22 s | — | — | coupé par un choc contre une structure |

- **Rotation en vol** :
  - Le tangage acquis au décollage continue : le nez passe de −4° à −22° en 1,1 s au-dessus d'une crête. Le roulis dérive peu (−15° → −23°).
  - |ω| s'amortit, avec une demi-vie de 0,21 s depuis 1,8 rad/s. La courbe observée suit dω/dt ≈ −(0,96 ω + 1,96 ω²).
- **Atterrissages** : tous ceux enregistrés sont des crashs (arêtes, flancs de colline), par exemple 203 → 60 km/h contre un flanc. Un seul se pose à peu près proprement sur neige (vy −5 m/s) : l'amortisseur avant droit est en butée (0,025) pendant 0,1 s, puis revient en environ 0,3 s.

## 6. Conduite réelle : `nadeo_finish`

Replay auteur de SnowB1 : 18,19 s, 3 checkpoints sur 3. Voir `plots/real_speed_hist.svg`.

- **Vitesse** : moyenne 170 km/h, max 242 km/h. 44 % du temps au-dessus de 200 km/h ; les pics sont à 100–160 km/h (44 %) et à 220–260 km/h (41 %).
- **Glisse** : p50 0,04°, p95 1,07°, max 2,1°. Aucune roue n'est en glisse (0 % du temps). La voiture n'est jamais en l'air (moins de 4 roues au sol 3,9 % du temps).
- **Accélération latérale** : p50 0,3 g, p95 11,3 g, max 20,3 g. Lacet : p95 1,88 rad/s, max 2,67 rad/s.
- **Direction** : braquage 20 % du temps, 26 appuis. Durée d'appui : médiane 80 ms, p90 215 ms, max 990 ms (épingle).
- **Gaz** 100 % du temps, **frein** 0 %.
- Clavier uniquement : aucun enregistrement de direction analogique n'existe.

## 7. Suspension (`wNdamper`)

Voir `plots/suspension.svg`.

- **Valeurs** : 0,5 = détendu (roue en l'air), **0,25 au repos**, 0,025 = butée (minimum 0,008 à 17 g). L'unité est probablement le mètre de débattement (non vérifié).
- **Chute au spawn** : période ≈ **0,5 s (2 Hz)**, amortissement ζ ≈ **0,26**. Extrema : 0,148 → 0,294 → 0,231 → 0,258.
- **Transfert de charge longitudinal** : aucun, sous 30 m/s² d'accélération comme sous 51 m/s² de freinage.
- **Roulis** (côté extérieur comprimé) :
  - 5,3° à 5,3–5,9 g : extérieur 0,17, intérieur 0,33 ;
  - 14,9° à 17,4 g : extérieur 0,009 (butée), intérieur 0,48 (roues intérieures presque décollées).
- **Sortie de rampe du pont** (pente −0,44 → 0 à 190–215 km/h) : minimum 0,082.
- **Atterrissage** : butée franche, voir la section 5.

---

## Plan : enregistrer la voiture Rally de TMUF sur terre (non exécuté)

Ce plan repose sur la lecture de `tools/tminterface/launch.py`, `run.py` et `ClaudeBridge.as`.

- **Chaîne existante** :
  - `launch.py <fichier carte>` démarre TMUF (TMLoader, bouteille CrossOver « TMUF ») jusqu'à une course sur la carte.
  - `run.py --map <id>` écrit pour chaque expérience un script de fenêtres « début fin action » (`up`, `down`, `left`, `right`, `steer=<±65536>`, `speed=`), le charge dans TMInterface et lance `@exp <nom> <durée>`.
  - Le plugin redémarre la course, injecte les entrées à chaque tick et écrit `rec_<nom>.csv`, avec le même format que ici (y compris `wNmat`).
  - Une carte s'ajoute via `MAPS = {id: (dossier, fichier .Challenge.Gbx, dossier de sortie, expériences)}`.
- **Carte** : il faut un challenge de l'environnement **Rally**, qui fournit la voiture Rally. Les tronçons de terre des cartes de campagne Nadeo sont courts, et leurs noms de fichiers sont à confirmer via GBX.NET ou la liste des pistes. Mieux vaut une **carte de test dédiée**, à générer avec GBX.NET pour ne rien cliquer dans le jeu, avec :
  - une ligne droite de terre plate d'au moins 800 m. La ligne droite de SnowB1 ne fait que 65 m, ce qui a obligé à passer par le run Nadeo pour les hautes vitesses ;
  - une aire de terre plate d'environ 100 × 100 m ;
  - une rampe en terre avec une zone d'atterrissage plate ;
  - des murs à 90° et à environ 20° ;
  - le même tracé en asphalte Rally, pour obtenir des rapports terre/asphalte.
- **Garde d'arrivée** : `setup()` calcule la zone d'arrivée avec `finish-box.ts`, qui a besoin de `data/<dir>/map.json`. Pour la carte de test, on peut placer l'arrivée hors d'atteinte et envoyer `@guard off`, ou passer une boîte mesurée à la main.
- **Expériences** : ce sont celles de `EXPERIMENTS`, préfixées `r_`, plus celles qui manquent au jeu Snow :
  - `accel` (15 s) ;
  - `coast` et `brake` depuis 60, 120 et 180 km/h ;
  - `reverse` ;
  - `steer_left`, `steer_right` et `steer_left_nogas` ;
  - `taps` ;
  - `circle20`, `circle35`, `circle50` et `circle65`, ainsi que `corner_fast`. Ils sont déjà définis dans `run.py` mais **jamais enregistrés sur Snow**, ce qui laisse un trou pour le braquage établi sous 80 km/h ;
  - `analog_full`, `analog_half` et `analog_steps`, pour la direction analogique ;
  - donuts et demi-tour sur l'aire ;
  - rampe et saut ;
  - `wall_head`, `wall_side` et un frottement le long d'un mur ;
  - transitions terre → asphalte et terre → herbe.
- **Contrôles avant d'exploiter les données** :
  1. Vérifier que les colonnes `gas`, `brake` et `steer` suivent le script. Les enregistrements `lpc/` ont échoué sur ce point (gas = 0) ; il faut ajouter une assertion dans `run.py`.
  2. Relever l'identifiant `wNmat` de la terre.
  3. Rejouer une expérience deux fois pour vérifier le déterminisme.
  4. Relancer `analyze.py` (sections ligne droite, direction et « snow » réutilisées avec le matériau terre).

---

## Cibles clés

| # | Cible | Valeur mesurée | Source |
|---|---|---|---|
| 1 | Accélération plein gaz (route) | 30 / 17,85 / 9 / 5 / 2,5 / 1 m/s², seuils 55 / 80 / 115 / 140 / 240 km/h, marches franches | `accel`, `nadeo_finish` |
| 2 | 0→100 / 0→200 km/h | 1,53 s (26,6 m) / ≈ 10,0 s (≈ 404 m) | `accel` (mesuré) / dérivé |
| 3 | Vitesse max | > 242 km/h (non atteinte, encore +1 m/s²) | `nadeo_finish` |
| 4 | Roue libre | −1,0 m/s², route et neige | `coast`, `x_snow_*` |
| 5 | Frein (route) | −51 m/s² ; −80 sous 33 km/h ; 96→0 en 6,8 m et 0,46 s | `brake` |
| 6 | Marche arrière | 30 m/s², plafond 150 km/h (route) et 30 km/h (neige) | `brake`, `reverse` |
| 7 | Pente | −30 m/s² par unité de sinus (plein gaz) | rampes du pont |
| 8 | Réponse de la direction | roues 0→1 en 50 ms (+10 ms) ; lacet t63 0,15 s ; relâché ×0,92 par tick | `taps` |
| 9 | Braquage max (route) | lacet 2,35→2,66 rad/s de 78 à 228 km/h ; R ≈ 9,5 m sous 80 km/h, 23,7 m à 228 | `steer_*`, `nadeo_finish` |
| 10 | Adhérence route | glisse < 2° (p95 ≤ 1,2° au-dessus de 80 km/h) jusqu'à 17 g ; virage = −5 à −7 m/s² | `nadeo_finish`, `steer_*` |
| 11 | Neige | traction ×0,5–0,6 ; a_lat max ≈ 2,5 g ; glisse jusqu'à 80° (tête-à-queue) ; freinage ≈ −21 m/s² | `x_snow_*` |
| 12 | Murs | frontal : 5–25 % conservés, rebond ≤ 0,7 m/s ; ~45° : 64–70 % ; frottement 10–16° : 86–89 % | `x_wall_*`, `x_cp_post` |
| 13 | Air | g = 40 m/s² (4,08 g), aucune traînée, rotation amortie (demi-vie 0,2 s) | `x_crest_off`, `x_ramp_off_*` |
| 14 | Suspension | repos 0,25 / détendu 0,5 / butée 0,025 ; 2 Hz, ζ 0,26 ; pas de tangage ; roulis 5° à 5 g, 15° à 17 g | `accel`, `brake`, `nadeo_finish` |
| 15 | Conduite réelle | 170 km/h de moyenne, 44 % du temps au-dessus de 200 ; braquage 20 % du temps (appuis de 80 ms en médiane) | `nadeo_finish` |
