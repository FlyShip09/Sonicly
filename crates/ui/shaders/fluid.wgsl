// Fluid: four colours of a cover carried along one slow current, like ink folding into water.
// The plane is warped twice by its own low noise, which draws long curling streams rather than
// the round pools of Waves, and the streams are coloured by where the second warp lands. The
// music only reaches the warp's strength and a faint lift in light, eased elsewhere, so the
// field breathes with a loud passage and never pulses with a beat.
//
// params[0].x  speed, 1 for the default pace
// params[0].y  energy, how loud the music is right now, eased, 0 to 1
// params[0].z  density, how many units of the field fit in the geometric mean of the sides
// params[0].w  ceiling, the sRGB luma above which light is pressed back, 0 for none
// params[1]    the stage the streams run over, straight alpha sRGB
// params[2].x  vignette, how much darker the corners are than the middle
// colors[0..3] the four stream colours, straight alpha sRGB, deepest first

// Two octaves of gradient noise, enough for a current with some body and cheap enough to run
// three times a pixel.
fn current(p: vec2<f32>) -> f32 {
    return gradient_noise(p) * 0.66 + gradient_noise(rotate(p, 0.9) * 2.1 + vec2<f32>(3.7, -1.9)) * 0.34;
}

fn shade(input: ShaderInput) -> vec4<f32> {
    let t = input.time * input.params[0].x;
    let energy = clamp(input.params[0].y, 0.0, 1.0);
    let density = max(input.params[0].z, 0.1);
    let p = (input.position - input.size * 0.5) / max(sqrt(input.size.x * input.size.y), 1.0) * density;

    // The current moves the whole plane one way while the warps fold it, so the streams travel
    // as well as curl.
    let drift = vec2<f32>(t * 0.018, -t * 0.011);
    let reach = 0.85 + 0.25 * energy;
    let first = vec2<f32>(
        current(p + drift + vec2<f32>(0.0, t * 0.021)),
        current(p + drift + vec2<f32>(5.2, 1.3) - vec2<f32>(t * 0.017, 0.0)),
    );
    let second = vec2<f32>(
        current(p + reach * first + vec2<f32>(1.7, 9.2) + vec2<f32>(t * 0.013, 0.0)),
        current(p + reach * first + vec2<f32>(8.3, 2.8) - vec2<f32>(0.0, t * 0.015)),
    );
    let field = current(p + reach * second);

    // Each colour takes a band of the field, with soft edges that overlap, mixed in Oklab so
    // two hues meet through their own shade rather than through grey.
    let level = clamp(field * 0.5 + 0.5, 0.0, 1.0);
    var color = input.params[1].rgb;
    color = mix_oklab(color, input.colors[0].rgb, smoothstep(0.05, 0.45, level) * 0.85);
    color = mix_oklab(color, input.colors[1].rgb, smoothstep(0.30, 0.65, level) * 0.8);
    color = mix_oklab(color, input.colors[2].rgb, smoothstep(0.5, 0.8, level) * 0.85);
    // The fourth runs along the folds, where the second warp turns hardest.
    let fold = smoothstep(0.25, 0.75, length(second));
    color = mix_oklab(color, input.colors[3].rgb, fold * 0.7);
    color *= 1.0 + 0.08 * energy;

    let edge = length((input.uv - vec2<f32>(0.5)) * 2.0) * 0.7071;
    color *= 1.0 - input.params[2].x * smoothstep(0.2, 1.0, edge);

    // The same soft ceiling as Waves, so a vivid cover never washes out the lines over it.
    let ceiling = input.params[0].w;
    let luma = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
    if ceiling > 0.0 && luma > ceiling {
        color *= (ceiling + (luma - ceiling) * 0.35) / luma;
    }

    return vec4<f32>(dither(color, input.frag_coord), 1.0);
}
