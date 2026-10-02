# Musique : direction artistique et prompts Suno

Validée le 2 octobre 2026. Les morceaux retenus sont ici (`theme.m4a`, `red_frontier.m4a`,
`dust_devil.m4a`, `night_shift.m4a`, tels que Suno les donne) ; `tools/audio/music.py` en fait
les fichiers que le jeu embarque.

## L'idée

Le jeu dit « chaque planète son propre style de conduite » : la musique suit la même règle. Un
son commun à tout le jeu, et une couleur propre à chaque planète posée dessus. Le menu joue le
socle pur, chaque planète le colore.

Références (pour nous ; Suno refuse les noms d'artistes) : la big beat des jeux de course
futuristes de la fin des années 90 (Wipeout, Chemical Brothers, Prodigy), l'électro des
Trackmania, les basses saturées de Justice, l'ampleur spatiale de *Tron Legacy*.

### Le socle commun

- **Instrumental** : on relance une course cinquante fois, une voix lasse vite ; le jeu est
  international.
- **Le moteur rythmique** : breakbeats lourds, basse synthé saturée, arpèges analogiques ;
  128 à 160 BPM.
- **L'espace** : nappes larges, montées filtrées, réverbe longue dans les breaks.
- **Un motif signature** : un hook de cinq notes né dans le thème du menu, repris par les
  planètes (fonction *Cover* de Suno).
- **Pensé pour la course** : ça démarre fort (intro de 8 s au plus), énergie stable, breaks
  courts ; de la place pour le moteur (100 à 250 Hz) : riffs en ponctuation, pas de mur de
  guitares ; 2:30 à 3:30, prêts à boucler.

### Mars 2036 : la frontière

Une colonie qui monte ses circuits avec ce qu'elle a (bâches, tubes de plastique rouge, sacs de
régolithe), dans le sable et le vent : guitare baryton twang et desert rock, grain de bande,
percussions trouvées (tubes frappés, cliquets de sangles, bâches qui claquent), souffles de
sable dans les transitions, mineur chaud avec une pointe de phrygien.

Plus tard : la planète de glace en cristal (cloches, verre, drum'n'bass plus rapide), la géante
gazeuse massive et flottante (sub énormes, psyché).

### Dans le jeu

Le thème joue seul sur tous les écrans du menu : le vent et la nappe d'ambiance du menu ont été
retirés, les deux avec la musique faisaient trop. En course, les morceaux de la
planète s'enchaînent et continuent pendant les restarts, à peu près au volume de la voiture.

## Réglages Suno

Mode Custom, Weirdness ~35 %, Style Influence ~75 %. La structure se colle dans le champ Lyrics
(si l'interface le cache avec *Instrumental* coché, le décocher et ne coller que les balises).
Garder la génération dont le hook s'entend dans les 15 premières secondes, sans voix, qui finit
net.

## Planet Tracks (le menu) : retenu

**Style**

    Instrumental space electro big beat, 128 BPM, D minor. Punchy breakbeat drums, saturated analog synth bass, sequenced analog arpeggios, wide cosmic pads, filtered risers and sweeps. A bright heroic synth lead plays a short, catchy 5-note hook that comes back as the main theme. Optimistic, adventurous, driving, futuristic racing across the solar system. Energetic from the first bar, steady momentum, short breakdowns. Deep sub bass and crisp highs, clean uncluttered midrange. No fade out, loopable.

**Exclure** : `vocals, singing, rap, spoken word, choir, orchestral, acoustic, guitar, lo-fi, ambient, chill, slow intro, trap, dubstep`

**Structure**

    [Intro: arpeggio and kick, filtered, 4 bars]
    [Main Theme: lead hook, full breakbeat]
    [Drop: distorted bass, hook doubled]
    [Break: cosmic pads, filtered riser, 8 bars]
    [Build: snare roll]
    [Drop: full energy, hook]
    [Main Theme]
    [Outro: hook, full drums, hard stop on the beat]

## Red Frontier (Mars, l'hymne) : retenu

En *Cover* du thème du menu, pour en garder la mélodie.

**Style**

    Instrumental desert rock meets electro big beat, 132 BPM, D minor. Twangy baritone guitar riffs with tremolo and spring reverb, tight breakbeat drums, gritty tape-saturated analog synths and arpeggios, deep sub bass. The catchy 5-note hook played on baritone guitar, answered by a synth lead. Found-sound percussion: struck plastic pipes, ratchet clicks, flapping tarp. Gusts of wind and sand in the transitions. Space western, frontier spirit, dusty sunset on Mars, driving racing energy. Short guitar riffs as punctuation, no wall of distorted guitars. Energetic from the first bar. No fade out, loopable.

**Exclure** : `vocals, singing, rap, spoken word, choir, orchestral, acoustic guitar, country, banjo, harmonica, metal, lo-fi, ambient, slow intro`

**Structure**

    [Intro: baritone guitar riff and breakbeat, 4 bars]
    [Main Theme: guitar hook, synth answer]
    [Drop: gritty bass, full drums, plastic pipe percussion]
    [Break: wind gust, spring reverb guitar, 8 bars]
    [Build: tom roll, riser]
    [Drop: hook on guitar and synth together]
    [Outro: riff, full drums, hard stop on the beat]

## Dust Devil (Mars, la plus nerveuse) : retenu

**Style**

    Instrumental aggressive big beat with fuzz guitar, 155 BPM, E minor with a phrygian desert flavour. Fast chopped breakbeats, squelchy distorted acid bassline, fuzz baritone guitar stabs, noise bursts, screaming filtered synth sweeps, whooshing sandstorm risers. Metallic and plastic found-sound percussion. Relentless and urgent: a high-speed chase through a Martian sandstorm. A short 5-note hook as a distorted guitar riff. Full energy from the first second, very short breakdowns, tight and punchy. Deep sub bass, crisp highs. No fade out, loopable.

**Exclure** : `vocals, singing, rap, spoken word, choir, orchestral, acoustic, dubstep, trap, metal growls, lo-fi, ambient, chill, slow intro`

**Structure**

    [Intro: breakbeat and acid bass, 2 bars]
    [Drop: fuzz guitar stabs, acid bass, full speed]
    [Main Theme: distorted guitar hook]
    [Break: sandstorm noise sweep, 4 bars]
    [Drop: double-time breakbeat, screaming synth]
    [Main Theme]
    [Outro: full drums, hard stop on the beat]

## Night Shift (Mars, la nuit) : retenu

Suno l'a mené à 8:00 et coupé en plein groove ; `music.py` le termine à 4:14, en fondu sur les
mesures calmes qui précèdent un groove répété jusqu'au bout.

**Style**

    Instrumental hypnotic dark electro, 126 BPM, A minor. Driving four-on-the-floor kick with shuffled breakbeat percussion, pulsing analog bass sequence, glassy arpeggios, cold wide pads. A distant twangy baritone guitar with long echo delay plays a short 5-note hook. Night on Mars: a colony working the night shift, work lights in a dark canyon, focused and tense, steady momentum for concentration. Soft wind texture. Deep sub bass, crisp hi-hats, sparse midrange. Starts with momentum. No fade out, loopable.

**Exclure** : `vocals, singing, rap, spoken word, choir, orchestral, piano ballad, lo-fi, ambient drone, chill, slow tempo, big room EDM`

**Structure**

    [Intro: kick and pulsing bass, 4 bars]
    [Groove: glassy arpeggios, shuffled percussion]
    [Main Theme: echoing baritone guitar hook]
    [Break: cold pads, wind, 8 bars]
    [Groove: filter opens, full drums]
    [Main Theme: hook with synth layer]
    [Outro: groove, hard stop on the beat]

## Liftoff (Mars, épique) : en option, pas encore faite

Possible aussi en *Cover* du thème du menu.

**Style**

    Instrumental epic electro rock, 140 BPM, D minor rising to a triumphant major chorus. Soaring synth lead and baritone guitar in unison playing a heroic 5-note hook, big breakbeat drums with huge tom fills, saturated synth bass, wide cosmic pads, tall risers before each drop, moments of weightless lift. Airborne and triumphant: huge jumps over Martian canyons, launching toward the stars. Energetic throughout, short builds. Deep sub bass, crisp highs. No fade out, loopable.

**Exclure** : `vocals, singing, rap, choir, orchestral, cinematic trailer, strings, lo-fi, ambient, slow intro`

**Structure**

    [Intro: guitar and synth hook in unison, drums, 4 bars]
    [Drop: big breakbeat, saturated bass]
    [Build: tom fills, riser]
    [Chorus: triumphant hook, major lift]
    [Break: weightless pads, 4 bars]
    [Chorus: hook, full energy]
    [Outro: hook, hard stop on the beat]

## Ajouter un morceau

1. Le déposer ici sous un nom court (`liftoff.m4a`).
2. Lui donner une entrée dans `TRACKS` (`tools/audio/music.py` : fondu de fin, coupe éventuelle)
   et lancer `/usr/bin/python3 tools/audio/music.py art/audio/music crates/app/assets/music liftoff`.
3. L'ajouter à la liste de sa planète dans `crates/app/src/music.rs` (`MARS`).
