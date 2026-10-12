-- Realer Xpad – Hyprland integration (Hyprland 0.55 and newer, Lua config)
-- Paste into ~/.config/hypr/hyprland.lua, or save next to it and add:
--   require("realer-xpad")      -- for ~/.config/hypr/realer-xpad.lua
-- Using hyprland.conf instead? See hyprland.conf in this folder.

local notes = [[^(ch\.giosuel\.RealerXpad)$]]

-- Notes float instead of tiling, and keep their own rounded look.
hl.window_rule({
    match = { class = notes },
    float = true,
    border_size = 0,
    rounding = 14,
    no_blur = true,
})

-- Start with Hyprland.
hl.on("hyprland.start", function()
    hl.exec_cmd("realer-xpad")
end)

-- Keybinds.
hl.bind("SUPER + N", hl.dsp.exec_cmd("realer-xpad new"))              -- new note
hl.bind("SUPER + SHIFT + N", hl.dsp.exec_cmd("realer-xpad toggle"))   -- hide / show all notes
hl.bind("SUPER + ALT + N", hl.dsp.exec_cmd("realer-xpad manager"))    -- notes manager
