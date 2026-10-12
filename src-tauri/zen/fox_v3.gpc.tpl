// ============================================================================
// MAJESTIC FOX V3 — generated script. DO NOT hand-edit the values below;
// they are baked in by the Majestic Fox app from your profiles. To change
// them, edit the profile in the app and re-export.
//
// Generated from: {{SOURCE_NAMES}}
// Generated at:   {{GENERATED_AT}}
// Profiles in this file: {{PRESET_COUNT}}
//
// HOW TO USE
//   1. Zen Studio -> Compiler -> open this file -> Compile -> drag to a
//      memory slot -> Program.
//   2. Switch weapon preset: hold VIEW and tap D-pad LEFT / RIGHT.
//      The OLED shows the active preset name.
//   3. Fine-tune: hold ADS (LT) and tap D-pad UP/DOWN (vertical) or
//      LEFT/RIGHT (horizontal). The OLED flashes the new numbers.
//      0-100 scale, same as the app.
//
// This is a clean, minimal Majestic Fox script (anti-recoil + deadzone +
// quick-nudge). It is NOT a modification of the Fox V2 custom script;
// that file is untouched. Test this in Zen Studio before trusting it live.
// ============================================================================

// ---- Baked-in profile data (from the app) ----
int PRESET_COUNT = {{PRESET_COUNT}};
const string PRESET_NAMES[] = { {{PRESET_NAMES}} };
int PRESET_V[] = { {{PRESET_V}} };   // vertical anti-recoil, 0-100
int PRESET_H[] = { {{PRESET_H}} };   // horizontal anti-recoil, 0-100 (pushes right)
int PRESET_DZ[] = { {{PRESET_DZ}} }; // stick deadzone, 0-100
// Response curve per preset: 0 = linear, 1 = dynamic, 2 = aggressive.
// v3 note: curves are a simplified sensitivity shaping, not a full curve lab.
int PRESET_CURVE[] = { {{PRESET_CURVE}} };

// ---- Live (tunable) state ----
int preset_idx = {{DEFAULT_PRESET}};
int ar_v;
int ar_h;
int deadzone;
int curve_kind;

init {
    ar_v = PRESET_V[preset_idx];
    ar_h = PRESET_H[preset_idx];
    deadzone = PRESET_DZ[preset_idx];
    curve_kind = PRESET_CURVE[preset_idx];
    printf("Fox V3 ready");
    printf(PRESET_NAMES[preset_idx]);
}

main {
    // ---- Deadzone: silence tiny stick drift ----
    int dx = get_val(XB1_RX);
    if (dx < deadzone && dx > -deadzone) {
        set_val(XB1_RX, 0);
    }
    int dy = get_val(XB1_RY);
    if (dy < deadzone && dy > -deadzone) {
        set_val(XB1_RY, 0);
    }

    // ---- Response curve: gentle sensitivity shaping ----
    // linear = untouched; dynamic = +10% low-end; aggressive = +20% low-end.
    if (curve_kind == 1) {
        set_val(XB1_RX, get_val(XB1_RX) + get_val(XB1_RX) / 10);
        set_val(XB1_RY, get_val(XB1_RY) + get_val(XB1_RY) / 10);
    }
    if (curve_kind == 2) {
        set_val(XB1_RX, get_val(XB1_RX) + get_val(XB1_RX) / 5);
        set_val(XB1_RY, get_val(XB1_RY) + get_val(XB1_RY) / 5);
    }

    // ---- Anti-recoil: while firing, counter the climb ----
    if (get_val(XB1_RT)) {
        int ry = get_val(XB1_RY) + ar_v;
        if (ry > 100) { ry = 100; }
        set_val(XB1_RY, ry);

        int rx = get_val(XB1_RX) + ar_h;
        if (rx > 100) { rx = 100; }
        if (rx < -100) { rx = -100; }
        set_val(XB1_RX, rx);
    }

    // ---- Preset switch: hold VIEW, tap D-pad LEFT/RIGHT ----
    if (get_val(XB1_VIEW)) {
        if (event_press(XB1_RIGHT)) {
            preset_idx = preset_idx + 1;
            if (preset_idx >= PRESET_COUNT) { preset_idx = 0; }
            apply_preset();
            printf(PRESET_NAMES[preset_idx]);
        }
        if (event_press(XB1_LEFT)) {
            preset_idx = preset_idx - 1;
            if (preset_idx < 0) { preset_idx = PRESET_COUNT - 1; }
            apply_preset();
            printf(PRESET_NAMES[preset_idx]);
        }
    }

    // ---- Quick-nudge: hold ADS (LT), tap D-pad to fine-tune ----
    if (get_val(XB1_LT)) {
        if (event_press(XB1_UP)) {
            ar_v = ar_v + 1;
            if (ar_v > 100) { ar_v = 100; }
            printf("V:%d H:%d", ar_v, ar_h);
        }
        if (event_press(XB1_DOWN)) {
            ar_v = ar_v - 1;
            if (ar_v < 0) { ar_v = 0; }
            printf("V:%d H:%d", ar_v, ar_h);
        }
        if (event_press(XB1_RIGHT)) {
            ar_h = ar_h + 1;
            if (ar_h > 100) { ar_h = 100; }
            printf("V:%d H:%d", ar_v, ar_h);
        }
        if (event_press(XB1_LEFT)) {
            ar_h = ar_h - 1;
            if (ar_h < 0) { ar_h = 0; }
            printf("V:%d H:%d", ar_v, ar_h);
        }
    }
}

// Load the active preset's baked-in values into the live state.
function apply_preset() {
    ar_v = PRESET_V[preset_idx];
    ar_h = PRESET_H[preset_idx];
    deadzone = PRESET_DZ[preset_idx];
    curve_kind = PRESET_CURVE[preset_idx];
}
