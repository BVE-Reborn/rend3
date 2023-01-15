fn shadow_sample_pcf5(tex: texture_depth_2d, samp: sampler_comparison, coords: vec2<f32>, depth: f32) -> f32 {
    var result: f32 = 0.0;
    result = result + textureSampleCompareLevel(tex, samp, coords, depth);
    result = result + textureSampleCompareLevel(tex, samp, coords, depth, vec2<i32>( 0,  1));
    result = result + textureSampleCompareLevel(tex, samp, coords, depth, vec2<i32>( 0, -1));
    result = result + textureSampleCompareLevel(tex, samp, coords, depth, vec2<i32>( 1,  0));
    result = result + textureSampleCompareLevel(tex, samp, coords, depth, vec2<i32>(-1,  0));
    return result * 0.2;
}
