// Menu background: the game's footage, framed to cover the screen, or the Martian night sky
// (gradient, horizon glow, stars, drifting dust) where it does not play; the static of a planet
// still to come; then the planets, each a sphere whose surface is computed per pixel from 3D
// noise on the unit sphere (no texture, so it stays sharp at any size). Writes sRGB values
// straight to the gamma target.

struct Planet {
    // centre x, y (px), radius (px), rotation (radians)
    a: vec4<f32>,
    // kind (0 Mars, 1 ice, 2 gas giant), dimmed (0..1), ring strength, opacity
    b: vec4<f32>,
    // clip rectangle x0, y0, x1, y1 (px)
    clip: vec4<f32>,
    // rim light colour, halo strength
    rim: vec4<f32>,
    // halo colour, halo extent (in radii; 0 = none)
    halo: vec4<f32>,
};

struct Sky {
    // width, height (px), time (s), pixels per point
    size: vec4<f32>,
    // horizon glow: centre x, y, radius x, y (px)
    glow: vec4<f32>,
    // parallax, planet count
    misc: vec4<f32>,
    // footage opacity, darkening, footage width and height (px)
    video: vec4<f32>,
    // static colour, strength
    noise: vec4<f32>,
    planets: array<Planet, 6>,
    // unit vector, angular radius
    craters: array<vec4<f32>, 64>,
};

@group(0) @binding(0) var<uniform> sky: Sky;
@group(0) @binding(1) var video_tex: texture_2d<f32>;
@group(0) @binding(2) var video_smp: sampler;

const D2R: f32 = 0.017453292;
const SKY_TOP: vec3<f32> = vec3<f32>(13.0, 9.0, 11.0) / 255.0;
const NIGHT: vec3<f32> = vec3<f32>(20.0, 13.0, 15.0) / 255.0;
const HORIZON: vec3<f32> = vec3<f32>(74.0, 31.0, 18.0) / 255.0;
const HORIZON_2: vec3<f32> = vec3<f32>(36.0, 18.0, 15.0) / 255.0;
const DUST: vec3<f32> = vec3<f32>(244.0, 232.0, 220.0) / 255.0;
const DIRT: vec3<f32> = vec3<f32>(194.0, 122.0, 72.0) / 255.0;
const GAS: vec3<f32> = vec3<f32>(227.0, 171.0, 97.0) / 255.0;
const GAS_2: vec3<f32> = vec3<f32>(156.0, 107.0, 60.0) / 255.0;
// Olympus Mons, 18.65° N 133.8° W.
const OLYMPUS: vec3<f32> = vec3<f32>(-0.6839, 0.3198, -0.6558);

@vertex
fn vs_menu(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

fn hash3(p: vec3<i32>) -> f32 {
    var h = (bitcast<u32>(p.x) * 374761393u) ^ (bitcast<u32>(p.y) * 668265263u) ^ (bitcast<u32>(p.z) * 1274126177u);
    h = (h ^ (h >> 13u)) * 1274126177u;
    h = h ^ (h >> 16u);
    return f32(h) * (1.0 / 4294967295.0);
}

fn vnoise(x: vec3<f32>) -> f32 {
    let i = vec3<i32>(floor(x));
    let f = fract(x);
    let u = f * f * (3.0 - 2.0 * f);
    let c000 = hash3(i);
    let c100 = hash3(i + vec3<i32>(1, 0, 0));
    let c010 = hash3(i + vec3<i32>(0, 1, 0));
    let c110 = hash3(i + vec3<i32>(1, 1, 0));
    let c001 = hash3(i + vec3<i32>(0, 0, 1));
    let c101 = hash3(i + vec3<i32>(1, 0, 1));
    let c011 = hash3(i + vec3<i32>(0, 1, 1));
    let c111 = hash3(i + vec3<i32>(1, 1, 1));
    return mix(mix(mix(c000, c100, u.x), mix(c010, c110, u.x), u.y), mix(mix(c001, c101, u.x), mix(c011, c111, u.x), u.y), u.z);
}

fn fbm(p: vec3<f32>, octaves: i32) -> f32 {
    var s = 0.0;
    var a = 0.5;
    var f = 1.0;
    var n = 0.0;
    for (var i = 0; i < octaves; i++) {
        let fi = f32(i);
        s += a * vnoise(p * f + vec3<f32>(fi * 17.3, -fi * 9.1, fi * 4.7));
        n += a;
        a *= 0.5;
        f *= 2.03;
    }
    return s / n;
}

// smoothstep that falls from 1 to 0 between `a` and `b` (a < b).
fn fall(a: f32, b: f32, x: f32) -> f32 {
    return 1.0 - smoothstep(a, b, x);
}

fn mars(p: vec3<f32>, lat_d: f32, lon_d: f32) -> vec3<f32> {
    let n1 = fbm(p * 1.6, 5);
    let n2 = fbm(p * 6.0, 4);
    let n3 = fbm(p * 22.0, 3);
    var c = vec3<f32>(0.80 + (n2 - 0.5) * 0.35, 0.43 + (n2 - 0.5) * 0.2, 0.25 + (n2 - 0.5) * 0.12);
    c = mix(c, vec3<f32>(0.36, 0.19, 0.13), smoothstep(0.5, 0.62, n1) * 0.7);
    c = mix(c, vec3<f32>(0.9, 0.6, 0.38), fall(0.32, 0.45, n1) * 0.5);
    var m = 0.9 + (n3 - 0.5) * 0.25;
    for (var i = 0; i < 60; i++) {
        let cr = sky.craters[i];
        let d = dot(p, cr.xyz);
        if d < cos(cr.w * 1.3) {
            continue;
        }
        let t = acos(min(1.0, d)) / cr.w;
        // A darker floor and a faint raised rim.
        m *= (1.0 - 0.13 * fall(0.7, 0.95, t)) * (1.0 + 0.06 * smoothstep(0.85, 1.0, t) * fall(1.0, 1.25, t));
    }
    let od = acos(min(1.0, dot(p, OLYMPUS)));
    if od < 0.11 {
        let t = od / 0.11;
        m *= select(1.0 + 0.18 * (1.0 - t), 0.72, t < 0.13);
    }
    // Valles Marineris, a long dark canyon south of the equator.
    if lon_d > -85.0 && lon_d < -30.0 {
        let d = abs(lat_d - (-8.0 + 0.8 * sin(lon_d * D2R * 4.0)));
        let fade = smoothstep(-85.0, -75.0, lon_d) * fall(-40.0, -30.0, lon_d);
        m *= 1.0 - 0.25 * fade * fall(0.4, 2.2, d);
    }
    c *= m;
    let w = (n2 - 0.5);
    let cap = max(smoothstep(78.0 + w * 16.0, 83.0 + w * 16.0, lat_d), fall(-85.0 - w * 14.0, -81.0 - w * 14.0, lat_d));
    return mix(c, vec3<f32>(0.95, 0.91, 0.87), cap);
}

fn ice(p: vec3<f32>) -> vec3<f32> {
    let n2 = fbm(p * 5.0, 4);
    let ridge = 1.0 - abs(fbm(p * 3.2, 4) * 2.0 - 1.0);
    var c = vec3<f32>(0.62 + (n2 - 0.5) * 0.18, 0.82 + (n2 - 0.5) * 0.14, 0.95 + (n2 - 0.5) * 0.08);
    c = mix(c, vec3<f32>(0.62, 0.36, 0.30), smoothstep(0.92, 0.98, ridge) * 0.8);
    var m = 1.0;
    for (var i = 40; i < 58; i++) {
        let cr = sky.craters[i];
        let d = dot(p, cr.xyz);
        if d < cos(cr.w * 1.3) {
            continue;
        }
        let t = acos(min(1.0, d)) / cr.w;
        if t < 0.9 {
            m *= 0.9;
        } else if t < 1.2 {
            m *= 1.06;
        }
    }
    return c * m;
}

fn gas(p: vec3<f32>, lat: f32, lat_d: f32, lon_d: f32) -> vec3<f32> {
    let n = fbm(p * 2.5, 4);
    let band = sin(lat * 11.0 + (n - 0.5) * 6.0) * 0.5 + 0.5;
    var c = vec3<f32>(mix(0.61, 0.89, band), mix(0.42, 0.67, band), mix(0.24, 0.38, band));
    let st = length(vec2<f32>(lat_d + 22.0, (lon_d - 40.0) * 0.5));
    if st < 9.0 {
        c = mix(c, vec3<f32>(0.78, 0.40, 0.22), (1.0 - st / 9.0) * 0.8);
    }
    return c;
}

fn dimmed(c: vec3<f32>) -> vec3<f32> {
    let l = dot(c, vec3<f32>(0.3, 0.55, 0.15));
    return vec3<f32>((c.x + l) * 0.5 * 0.27, (c.y + l) * 0.5 * 0.27, (c.z + l) * 0.5 * 0.3);
}

struct RingHit {
    col: vec3<f32>,
    a: f32,
    // On the near half, in front of the planet.
    front: bool,
};

// The ring around a gas giant at `d` (offset from the centre, in radii): tilted towards the
// viewer and turned a little, three bands with gaps between them.
fn ring(d: vec2<f32>, r_px: f32) -> RingHit {
    let angle = -0.38;
    let tilt = 0.3;
    let cs = cos(angle);
    let sn = sin(angle);
    let lx = d.x * cs + d.y * sn;
    let ly = (-d.x * sn + d.y * cs) / tilt;
    let rr = length(vec2<f32>(lx, ly));
    let w = 1.2 / r_px;
    let b1 = smoothstep(1.32 - w, 1.32 + w, rr) * fall(1.52 - w, 1.52 + w, rr) * 0.5;
    let b2 = smoothstep(1.55 - w, 1.55 + w, rr) * fall(1.80 - w, 1.80 + w, rr) * 0.8;
    let b3 = smoothstep(1.86 - w, 1.86 + w, rr) * fall(2.06 - w, 2.06 + w, rr) * 0.55;
    let a = b1 + b2 + b3;
    let col = (GAS_2 * (b1 + b3) + GAS * b2) / max(a, 1e-4);
    let front = ly > 0.0;
    return RingHit(col, min(a, 1.0) * select(0.8, 1.0, front), front);
}

fn draw_planet(pl: Planet, px: vec2<f32>, dst: vec3<f32>) -> vec3<f32> {
    if px.x < pl.clip.x || px.y < pl.clip.y || px.x > pl.clip.z || px.y > pl.clip.w {
        return dst;
    }
    let r = pl.a.z;
    let d = (px - pl.a.xy) / r;
    let dist2 = dot(d, d);
    let alpha = pl.b.w;
    var col = dst;
    let extent = pl.halo.w;
    if extent > 1.0 && dist2 < extent * extent {
        col = mix(col, pl.halo.rgb, pl.rim.w * max(0.0, 1.0 - sqrt(dist2) / extent) * alpha);
    }
    var front = vec4<f32>(0.0);
    if pl.b.z > 0.0 && dist2 < 4.4 {
        let rg = ring(d, r);
        let a = rg.a * pl.b.z * alpha;
        if rg.front {
            front = vec4<f32>(rg.col, a);
        } else {
            col = mix(col, rg.col, a);
        }
    }
    let lim = 1.0 + 2.0 / r;
    if dist2 < lim * lim {
        let dist = sqrt(dist2);
        let edge = clamp((1.0 - dist) * r + 0.5, 0.0, 1.0);
        let s = select(1.0, 1.0 / dist, dist > 1.0);
        let nx = d.x * s;
        let ny = -d.y * s;
        let nz = sqrt(max(0.0, 1.0 - min(dist2, 1.0)));
        let lat = asin(clamp(ny, -1.0, 1.0));
        let lon = atan2(nx, nz) + pl.a.w;
        let p = vec3<f32>(cos(lat) * sin(lon), sin(lat), cos(lat) * cos(lon));
        let lat_d = lat / D2R;
        let lon_d = atan2(p.x, p.z) / D2R;
        let kind = i32(pl.b.x + 0.5);
        var base: vec3<f32>;
        if kind == 0 {
            base = mars(p, lat_d, lon_d);
        } else if kind == 1 {
            base = ice(p);
        } else {
            base = gas(p, lat, lat_d, lon_d);
        }
        base = mix(base, dimmed(base), pl.b.y);
        let l = normalize(vec3<f32>(-0.52, 0.46, 0.72));
        let diff = nx * l.x + ny * l.y + nz * l.z;
        let shade = (0.03 + 1.1 * smoothstep(-0.2, 0.6, diff)) * (0.72 + 0.28 * nz);
        let rim = pow(1.0 - nz, 2.4) * (0.12 + 0.88 * smoothstep(-0.4, 0.5, diff));
        col = mix(col, base * shade + rim * pl.rim.rgb, edge * alpha);
    }
    return mix(col, front.rgb, clamp(front.a, 0.0, 1.0));
}

fn hash2(c: vec2<f32>, layer: i32) -> f32 {
    return hash3(vec3<i32>(i32(c.x), i32(c.y), layer));
}

fn sky_colour(px: vec2<f32>) -> vec3<f32> {
    let size = sky.size.xy;
    let t = sky.size.z;
    let ppp = sky.size.w;
    var c = mix(SKY_TOP, NIGHT, clamp(px.y / size.y, 0.0, 1.0));
    // Horizon glow: an ellipse fading from the warm horizon colour to nothing.
    let e = length((px - sky.glow.xy) / max(sky.glow.zw, vec2<f32>(1.0)));
    if e < 0.38 {
        c = mix(HORIZON, HORIZON_2, e / 0.38);
    } else if e < 0.7 {
        c = mix(c, HORIZON_2, 1.0 - (e - 0.38) / 0.32);
    }
    // Three layers of stars at different depths, twinkling, drifting slowly and shifting with
    // the parallax as the menu moves from screen to screen.
    for (var layer = 0; layer < 3; layer++) {
        let z = 0.3 + 0.3 * f32(layer);
        let cell = (64.0 + 18.0 * f32(layer)) * ppp;
        let shift = (t * 2.5 * z - sky.misc.x * 60.0 * z) * ppp;
        let q = vec2<f32>(px.x - shift, px.y);
        let id = floor(q / cell);
        let h = hash2(id, layer * 7 + 1);
        if h > 0.72 {
            continue;
        }
        let star = (id + vec2<f32>(0.15 + 0.7 * hash2(id, layer * 7 + 2), 0.15 + 0.7 * hash2(id, layer * 7 + 3))) * cell;
        let radius = (0.35 + pow(hash2(id, layer * 7 + 4), 4.0) * 1.1) * ppp;
        let twinkle = 0.45 + 0.55 * (0.5 + 0.5 * sin(t * (0.4 + 1.8 * hash2(id, layer * 7 + 5)) + 6.28 * hash2(id, layer * 7 + 6)));
        let dd = length(q - star);
        let a = z * twinkle * fall(radius * 0.4, radius + 0.6 * ppp, dd) * 1.4;
        c = mix(c, DUST, clamp(a, 0.0, 1.0));
    }
    // Dust drifting across the screen.
    for (var layer = 0; layer < 2; layer++) {
        let cell = (120.0 + 40.0 * f32(layer)) * ppp;
        let speed = vec2<f32>(14.0 + 10.0 * f32(layer), 5.0 + 4.0 * f32(layer)) * ppp;
        let q = px - speed * t;
        let id = floor(q / cell);
        if hash2(id, 31 + layer) > 0.4 {
            continue;
        }
        let mote = (id + vec2<f32>(0.2 + 0.6 * hash2(id, 33 + layer), 0.2 + 0.6 * hash2(id, 35 + layer))) * cell;
        let radius = (0.6 + 1.4 * hash2(id, 37 + layer)) * ppp;
        let a = (0.08 + 0.16 * hash2(id, 39 + layer)) * fall(radius * 0.5, radius + 0.5 * ppp, length(q - mote));
        c = mix(c, DIRT, a);
    }
    return c;
}

@fragment
fn fs_menu(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let px = frag.xy;
    let size = sky.size.xy;
    var c = sky_colour(px);
    // The footage, scaled to cover the screen and centred.
    if sky.video.x > 0.0 {
        let vs = sky.video.zw;
        let k = max(size.x / vs.x, size.y / vs.y);
        let uv = ((px - size * 0.5) / k + vs * 0.5) / vs;
        c = mix(c, textureSampleLevel(video_tex, video_smp, uv, 0.0).rgb, sky.video.x);
    }
    // Static, coarse and tinted, with a brighter band rolling down.
    if sky.noise.w > 0.0 {
        let cell = max(size.x, size.y) / 170.0;
        let id = floor(px / cell);
        let t = sky.size.z;
        let roll = fract(t * 0.35) * size.y;
        let band = select(1.0, 1.6, abs(px.y - roll) < cell * 4.0);
        let h = hash3(vec3<i32>(i32(id.x), i32(id.y), i32(floor(t * 24.0)))) * 0.75 * band;
        c = mix(c, sky.noise.rgb * h, sky.noise.w);
    }
    c *= 1.0 - sky.video.y;
    let n = i32(sky.misc.y + 0.5);
    for (var i = 0; i < n; i++) {
        c = draw_planet(sky.planets[i], px, c);
    }
    // A little dither so the dark gradients do not band.
    let dither = (hash2(floor(px), 77) - 0.5) / 255.0;
    return vec4<f32>(c + dither, 1.0);
}
