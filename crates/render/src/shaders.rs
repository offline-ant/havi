//! GPU shader definitions: DrawRoundedColor, DrawBoxShadow, DrawGradient.

use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets.*

    set_type_default() do #(DrawRoundedColor::script_shader(vm)){
        ..mod.draw.DrawQuad
        border_radius_tl: 0.0
        border_radius_tr: 0.0
        border_radius_br: 0.0
        border_radius_bl: 0.0
        pixel: fn(){
            let sdf = Sdf2d.viewport(self.pos * self.rect_size)
            sdf.box_all(
                0.0
                0.0
                self.rect_size.x
                self.rect_size.y
                max(self.border_radius_tl, 0.5)
                max(self.border_radius_tr, 0.5)
                max(self.border_radius_br, 0.5)
                max(self.border_radius_bl, 0.5)
            )
            sdf.fill(self.color)
            return sdf.result
        }
    }

    set_type_default() do #(DrawGradient::script_shader(vm)){
        ..mod.draw.DrawQuad
        grad_type: 0.0
        repeating: 0.0
        param0: 0.0
        param1: 0.0
        param2: 1.0
        param3: 1.0
        stop_count: 2.0
        stop0_color: vec4(0.0, 0.0, 0.0, 1.0)
        stop0_pos: 0.0
        stop1_color: vec4(1.0, 1.0, 1.0, 1.0)
        stop1_pos: 1.0
        stop2_color: vec4(0.0, 0.0, 0.0, 0.0)
        stop2_pos: 0.0
        stop3_color: vec4(0.0, 0.0, 0.0, 0.0)
        stop3_pos: 0.0
        stop4_color: vec4(0.0, 0.0, 0.0, 0.0)
        stop4_pos: 0.0
        stop5_color: vec4(0.0, 0.0, 0.0, 0.0)
        stop5_pos: 0.0
        stop6_color: vec4(0.0, 0.0, 0.0, 0.0)
        stop6_pos: 0.0
        stop7_color: vec4(0.0, 0.0, 0.0, 0.0)
        stop7_pos: 0.0
        pixel: fn(){
            let uv = self.pos
            var t = 0.0
            if self.grad_type < 0.5 {
                // Linear gradient: project UV onto gradient line.
                let p0 = vec2(self.param0, self.param1)
                let p1 = vec2(self.param2, self.param3)
                let d = p1 - p0
                let len2 = dot(d, d)
                if len2 > 0.000001 {
                    t = dot(uv - p0, d) / len2
                }
            } else if self.grad_type < 1.5 {
                // Radial gradient: normalized distance from center.
                let cx = self.param0
                let cy = self.param1
                let rx = self.param2
                let ry = self.param3
                let dx = (uv.x - cx)
                let dy = (uv.y - cy)
                if rx > 0.0001 && ry > 0.0001 {
                    t = length(vec2(dx / rx, dy / ry))
                }
            } else {
                // Conic gradient: angle from center, normalized to [0,1].
                // param0/param1 = center (UV), param2 = start angle (radians).
                let cx = self.param0
                let cy = self.param1
                let start_angle = self.param2
                let dx = uv.x - cx
                let dy = uv.y - cy
                // atan(y,x) gives angle from positive x-axis (-PI..PI).
                // CSS conic: 0 is up (+y axis in screen = -y in math), clockwise.
                var angle = atan2(dx, -dy) - start_angle
                // Normalize to [0, 2*PI).
                let two_pi = 6.283185307
                angle = angle - floor(angle / two_pi) * two_pi
                t = angle / two_pi
            }
            // Repeating: wrap t into [first_stop, last_stop] range using fract.
            if self.repeating > 0.5 {
                let first = self.stop0_pos
                var last = self.stop1_pos
                if self.stop_count >= 3.0 { last = self.stop2_pos }
                if self.stop_count >= 4.0 { last = self.stop3_pos }
                if self.stop_count >= 5.0 { last = self.stop4_pos }
                if self.stop_count >= 6.0 { last = self.stop5_pos }
                if self.stop_count >= 7.0 { last = self.stop6_pos }
                if self.stop_count >= 8.0 { last = self.stop7_pos }
                let range = last - first
                if range > 0.00001 {
                    t = first + fract((t - first) / range) * range
                }
            } else {
                t = clamp(t, 0.0, 1.0)
            }
            var color = self.stop0_color
            if self.stop_count >= 2.0 {
                var c0 = self.stop0_color
                var p_0 = self.stop0_pos
                var c1 = self.stop1_color
                var p_1 = self.stop1_pos
                if self.stop_count >= 3.0 && t > self.stop1_pos {
                    c0 = self.stop1_color; p_0 = self.stop1_pos
                    c1 = self.stop2_color; p_1 = self.stop2_pos
                }
                if self.stop_count >= 4.0 && t > self.stop2_pos {
                    c0 = self.stop2_color; p_0 = self.stop2_pos
                    c1 = self.stop3_color; p_1 = self.stop3_pos
                }
                if self.stop_count >= 5.0 && t > self.stop3_pos {
                    c0 = self.stop3_color; p_0 = self.stop3_pos
                    c1 = self.stop4_color; p_1 = self.stop4_pos
                }
                if self.stop_count >= 6.0 && t > self.stop4_pos {
                    c0 = self.stop4_color; p_0 = self.stop4_pos
                    c1 = self.stop5_color; p_1 = self.stop5_pos
                }
                if self.stop_count >= 7.0 && t > self.stop5_pos {
                    c0 = self.stop5_color; p_0 = self.stop5_pos
                    c1 = self.stop6_color; p_1 = self.stop6_pos
                }
                if self.stop_count >= 8.0 && t > self.stop6_pos {
                    c0 = self.stop6_color; p_0 = self.stop6_pos
                    c1 = self.stop7_color; p_1 = self.stop7_pos
                }
                let range = p_1 - p_0
                var frac = 0.0
                if range > 0.00001 {
                    frac = clamp((t - p_0) / range, 0.0, 1.0)
                }
                color = mix(c0, c1, frac)
            }
            return vec4(color.rgb * color.a, color.a)
        }
    }

    set_type_default() do #(DrawBoxShadow::script_shader(vm)){
        ..mod.draw.DrawQuad
        shadow_color: vec4(0.0, 0.0, 0.0, 0.0)
        box_offset: vec2(0.0, 0.0)
        box_size: vec2(0.0, 0.0)
        sigma: 0.001
        corner: 0.0
        inset: 0.0
        pixel: fn(){
            let point = self.pos * self.rect_size
            let lower = self.box_offset
            let upper = self.box_offset + self.box_size
            var mask = GaussShadow.rounded_box_shadow(lower, upper, point, max(self.sigma, 0.001), self.corner)
            if self.inset > 0.5 {
                mask = 1.0 - mask
            }
            return vec4(self.shadow_color.rgb * self.shadow_color.a * mask, self.shadow_color.a * mask)
        }
    }

    set_type_default() do #(DrawFilterImage::script_shader(vm)){
        ..mod.draw.DrawQuad
        filter_texture: texture_2d(float)
        opacity: 1.0
        blur_radius: 0.0
        brightness: 1.0
        contrast: 1.0
        grayscale: 0.0
        hue_rotate: 0.0
        invert: 0.0
        saturate: 1.0
        sepia: 0.0
        tex_size: vec2(1.0, 1.0)
        pixel: fn(){
            let uv = self.pos
            var color = vec4(0.0, 0.0, 0.0, 0.0)
            if self.blur_radius > 0.5 {
                // 9-tap 2D Gaussian blur approximation.
                // Step size in UV space: blur_radius / texture_size.
                let sx = self.blur_radius / self.tex_size.x
                let sy = self.blur_radius / self.tex_size.y
                // Gaussian weights: center=1, cardinal=exp(-0.5)≈0.607, diagonal=exp(-1)≈0.368
                let w0 = 0.2042
                let w1 = 0.1240
                let w2 = 0.0752
                color = self.filter_texture.sample_as_bgra(uv) * w0
                color += self.filter_texture.sample_as_bgra(uv + vec2(sx, 0.0)) * w1
                color += self.filter_texture.sample_as_bgra(uv + vec2(-sx, 0.0)) * w1
                color += self.filter_texture.sample_as_bgra(uv + vec2(0.0, sy)) * w1
                color += self.filter_texture.sample_as_bgra(uv + vec2(0.0, -sy)) * w1
                color += self.filter_texture.sample_as_bgra(uv + vec2(sx, sy)) * w2
                color += self.filter_texture.sample_as_bgra(uv + vec2(-sx, sy)) * w2
                color += self.filter_texture.sample_as_bgra(uv + vec2(sx, -sy)) * w2
                color += self.filter_texture.sample_as_bgra(uv + vec2(-sx, -sy)) * w2
            } else {
                color = self.filter_texture.sample_as_bgra(uv)
            }
            // Unpremultiply alpha for color operations.
            if color.a > 0.001 {
                color = vec4(color.rgb / color.a, color.a)
            }
            // Grayscale
            if self.grayscale > 0.001 {
                let g = dot(color.rgb, vec3(0.2126, 0.7152, 0.0722))
                color = vec4(mix(color.rgb, vec3(g, g, g), self.grayscale), color.a)
            }
            // Sepia
            if self.sepia > 0.001 {
                let sr = min(dot(color.rgb, vec3(0.393, 0.769, 0.189)), 1.0)
                let sg = min(dot(color.rgb, vec3(0.349, 0.686, 0.168)), 1.0)
                let sb = min(dot(color.rgb, vec3(0.272, 0.534, 0.131)), 1.0)
                color = vec4(mix(color.rgb, vec3(sr, sg, sb), self.sepia), color.a)
            }
            // Saturate
            if abs(self.saturate - 1.0) > 0.001 {
                let g = dot(color.rgb, vec3(0.2126, 0.7152, 0.0722))
                color = vec4(mix(vec3(g, g, g), color.rgb, self.saturate), color.a)
            }
            // Hue-rotate (CSS hue-rotation color matrix)
            if abs(self.hue_rotate) > 0.001 {
                let rad = self.hue_rotate * 0.01745329
                let s = sin(rad)
                let c = cos(rad)
                let nr = color.r*(0.213+c*0.787-s*0.213) + color.g*(0.715-c*0.715-s*0.715) + color.b*(0.072-c*0.072+s*0.928)
                let ng = color.r*(0.213-c*0.213+s*0.143) + color.g*(0.715+c*0.285+s*0.140) + color.b*(0.072-c*0.072-s*0.283)
                let nb = color.r*(0.213-c*0.213-s*0.787) + color.g*(0.715-c*0.715+s*0.715) + color.b*(0.072+c*0.928+s*0.072)
                color = vec4(nr, ng, nb, color.a)
            }
            // Invert
            if self.invert > 0.001 {
                color = vec4(mix(color.rgb, vec3(1.0, 1.0, 1.0) - color.rgb, self.invert), color.a)
            }
            // Brightness
            color = vec4(color.rgb * self.brightness, color.a)
            // Contrast
            color = vec4((color.rgb - vec3(0.5, 0.5, 0.5)) * self.contrast + vec3(0.5, 0.5, 0.5), color.a)
            // Re-premultiply and apply opacity
            let fa = color.a * self.opacity
            return vec4(color.rgb * fa, fa)
        }
    }
}

/// CSS filter post-processing (blur, brightness, contrast, etc).
#[derive(Script, ScriptHook, Debug)]
#[repr(C)]
pub struct DrawFilterImage {
    #[deref]
    pub draw_super: DrawQuad,
    #[live(1.0)]
    pub opacity: f32,
    #[live]
    pub blur_radius: f32,
    #[live(1.0)]
    pub brightness: f32,
    #[live(1.0)]
    pub contrast: f32,
    #[live]
    pub grayscale: f32,
    #[live]
    pub hue_rotate: f32,
    #[live]
    pub invert: f32,
    #[live(1.0)]
    pub saturate: f32,
    #[live]
    pub sepia: f32,
    #[live]
    pub tex_size: Vec2f,
}

impl DrawFilterImage {
    pub fn draw_abs(&mut self, cx: &mut Cx2d, rect: Rect) {
        self.draw_super.rect_pos = rect.pos.into();
        self.draw_super.rect_size = rect.size.into();
        self.draw_super.draw(cx);
    }
}

/// SDF-based per-corner border-radius.
#[derive(Script, ScriptHook, Debug)]
#[repr(C)]
pub struct DrawRoundedColor {
    #[deref]
    pub draw_super: DrawQuad,
    #[live]
    pub color: Vec4f,
    #[live]
    pub border_radius_tl: f32,
    #[live]
    pub border_radius_tr: f32,
    #[live]
    pub border_radius_br: f32,
    #[live]
    pub border_radius_bl: f32,
}

impl DrawRoundedColor {
    pub fn draw_abs(&mut self, cx: &mut Cx2d, rect: Rect) {
        self.draw_super.rect_pos = rect.pos.into();
        self.draw_super.rect_size = rect.size.into();
        self.draw_super.draw(cx);
    }
}

/// GPU-evaluated CSS gradient (linear, radial, conic; up to 8 color stops).
#[derive(Script, ScriptHook, Debug)]
#[repr(C)]
pub struct DrawGradient {
    #[deref]
    pub draw_super: DrawQuad,
    #[live] pub grad_type: f32,
    #[live] pub repeating: f32,
    #[live] pub param0: f32,
    #[live] pub param1: f32,
    #[live] pub param2: f32,
    #[live] pub param3: f32,
    #[live] pub stop_count: f32,
    #[live] pub stop0_color: Vec4f, #[live] pub stop0_pos: f32,
    #[live] pub stop1_color: Vec4f, #[live] pub stop1_pos: f32,
    #[live] pub stop2_color: Vec4f, #[live] pub stop2_pos: f32,
    #[live] pub stop3_color: Vec4f, #[live] pub stop3_pos: f32,
    #[live] pub stop4_color: Vec4f, #[live] pub stop4_pos: f32,
    #[live] pub stop5_color: Vec4f, #[live] pub stop5_pos: f32,
    #[live] pub stop6_color: Vec4f, #[live] pub stop6_pos: f32,
    #[live] pub stop7_color: Vec4f, #[live] pub stop7_pos: f32,
}

impl DrawGradient {
    pub fn draw_abs(&mut self, cx: &mut Cx2d, rect: Rect) {
        self.draw_super.rect_pos = rect.pos.into();
        self.draw_super.rect_size = rect.size.into();
        self.draw_super.draw(cx);
    }
}

/// GPU-accelerated box shadow using Makepad's GaussShadow Gaussian blur.
#[derive(Script, ScriptHook, Debug)]
#[repr(C)]
pub struct DrawBoxShadow {
    #[deref]
    pub draw_super: DrawQuad,
    #[live] pub shadow_color: Vec4f,
    #[live] pub box_offset: Vec2f,
    #[live] pub box_size: Vec2f,
    #[live] pub sigma: f32,
    #[live] pub corner: f32,
    #[live] pub inset: f32,
}

impl DrawBoxShadow {
    pub fn draw_abs(&mut self, cx: &mut Cx2d, rect: Rect) {
        self.draw_super.rect_pos = rect.pos.into();
        self.draw_super.rect_size = rect.size.into();
        self.draw_super.draw(cx);
    }
}


