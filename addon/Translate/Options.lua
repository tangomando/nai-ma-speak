local _, ns = ...

-- The panel under Options > AddOns. Every control writes straight to
-- TranslateDB, the same values the /tr commands toggle.

local ALL = "" -- sourceWindow value that sends every line

-- Choices for "Lines sent to the overlay": all chat, or one entry per chat
-- window. Built each time the dropdown opens, so renamed and new windows
-- show up. A saved window that no longer exists stays listed, marked.
local function SourceOptions()
    local container = Settings.CreateControlTextContainer()
    container:Add(ALL, "All chat")
    local saved, found = ns.db.sourceWindow, false
    for _, name in ipairs(ns.ChatWindowNames()) do
        container:Add(name, "Copy the \"" .. name .. "\" window")
        found = found or name == saved
    end
    if type(saved) == "string" and saved ~= ALL and not found then
        container:Add(saved, "\"" .. saved .. "\" (no such window: all chat is sent)")
    end
    return container:GetData()
end

local function Build()
    local category = Settings.RegisterVerticalLayoutCategory("Nai Ma Speak")

    local function Checkbox(key, label, tooltip)
        local setting = Settings.RegisterAddOnSetting(category, "Translate_" .. key, key, ns.db,
            Settings.VarType.Boolean, label, true)
        Settings.CreateCheckbox(category, setting, tooltip)
        return setting
    end

    local source = Settings.RegisterAddOnSetting(category, "Translate_sourceWindow", "sourceWindow", ns.db,
        Settings.VarType.String, "Lines sent to the overlay", ALL)
    Settings.CreateDropdown(category, source, SourceOptions,
        "Send player chat and the game's own messages as they arrive, or copy one chat window: "
        .. "every line it shows, worded and colored the same, including what other addons print. "
        .. "Pick the message types and channels in that window's own settings "
        .. "(right-click its tab, Settings). Its tab does not have to be selected, "
        .. "so a spare tab can serve just to make the selection.")

    local repeated = Settings.RegisterAddOnSetting(category, "Translate_repeatLines", "repeatLines", ns.db,
        Settings.VarType.Number, "Lines shown to the companion again", 50)
    Settings.CreateDropdown(category, repeated, function()
        local container = Settings.CreateControlTextContainer()
        container:Add(0, "None")
        for _, count in ipairs({ 20, 50, 100, 200 }) do container:Add(count, "The last " .. count) end
        return container:GetData()
    end, "While no new line waits, the last chat lines are sent to the companion again and again. "
        .. "Lines it missed, because the game was covered by another window or the companion was not running, "
        .. "then appear in the overlay in their place within seconds. Lines it already has are not shown twice.")

    local buttons = Settings.RegisterAddOnSetting(category, "Translate_buttons", "buttons", ns.db,
        Settings.VarType.String, "Translate and App buttons", "tabs")
    Settings.CreateDropdown(category, buttons, function()
        local container = Settings.CreateControlTextContainer()
        container:Add("tabs", "On the tab row")
        container:Add("top", "Beside the chat window, top right")
        container:Add("bottom", "Beside the chat window, bottom right")
        container:Add("hidden", "Hidden")
        return container:GetData()
    end, "Where the two buttons sit on the main chat window. Translate opens and closes the box for typing a reply; "
        .. "App shows and hides the companion app's window over the game.")
    Settings.SetOnValueChangedCallback("Translate_buttons", function() ns.PlaceButtons() end, ns)

    Checkbox("allLines", "Include lines without Chinese",
        "Also send lines without Chinese. The overlay shows them untranslated, in order with the translated ones.")
    Checkbox("glossary", "Glossary notes in chat",
        "After WoW terms in Chinese chat, add their English in grey: 缺坦 (need tank). A line said often gets its whole "
        .. "English, from translations the companion remembers. The companion adds to the glossary as it translates; "
        .. "/reload to load what it has added.")
    Checkbox("showLinks", "[T], [?] and [?N] links after chat lines",
        "Add links after lines in the game's chat windows. After a Chinese message: [T] sends it to the overlay "
        .. "to be translated again, for a line that scrolled away or was dropped when chat was too busy, and "
        .. "[?] has it explained word by word, for learning Chinese. After a line from a player with a Chinese "
        .. "name: [?N] explains the name. All go ahead of everything waiting.")

    Settings.RegisterAddOnCategory(category)
    return category
end

function ns.InitOptions()
    if not (Settings and Settings.RegisterVerticalLayoutCategory and Settings.RegisterAddOnSetting
            and Settings.CreateDropdown and Settings.CreateCheckbox) then
        ns.optionsSource = "none (no Settings API)"
        return
    end
    -- The Settings API differs between client versions; a failure here must
    -- not take the rest of the addon down with it.
    local ok, result = pcall(Build)
    if not ok then
        ns.optionsSource = "failed: " .. tostring(result)
        return
    end
    ns.optionsSource = "Settings API"
    function ns.OpenOptions()
        Settings.OpenToCategory(result:GetID())
    end
end
