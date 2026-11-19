local addonName, ns = ...

-- One event frame, one handler table keyed by event name.
ns.handlers = {}
local frame = CreateFrame("Frame")
ns.eventFrame = frame

function ns.On(event, fn)
    ns.handlers[event] = fn
    frame:RegisterEvent(event)
end

frame:SetScript("OnEvent", function(_, event, ...)
    local fn = ns.handlers[event]
    if fn then fn(...) end
end)

local DEFAULTS = {
    allLines = true,  -- also send lines without Chinese, shown untranslated in the overlay
    showLinks = true, -- add links after chat lines: translate again, explain, explain the name
    sourceWindow = "", -- tab name of the chat window whose settings pick the lines sent; "" sends all
    repeatLines = 50, -- how many of the last chat lines the strip shows again while nothing new waits
    buttons = "tabs", -- where the Translate and App buttons sit: "tabs", "top", "bottom" or "hidden"
}

function ns.Print(msg)
    DEFAULT_CHAT_FRAME:AddMessage("|cff66ccffNai Ma Speak:|r " .. msg)
end

-- Secret values (Midnight) cannot be compared or concatenated.
function ns.IsSecret(value)
    return issecretvalue ~= nil and issecretvalue(value) == true
end

ns.On("ADDON_LOADED", function(name)
    if name ~= addonName then return end
    TranslateDB = TranslateDB or {}
    for k, v in pairs(DEFAULTS) do
        if TranslateDB[k] == nil then TranslateDB[k] = v end
    end
    ns.db = TranslateDB
    frame:UnregisterEvent("ADDON_LOADED")
end)

ns.On("PLAYER_LOGIN", function()
    if ns.InitChat then ns.InitChat() end
    if ns.InitLayout then ns.InitLayout() end
    if ns.InitButtons then ns.InitButtons() end
    if ns.InitOptions then ns.InitOptions() end
end)

-- The most lines "/tr test <count>" sends: 1 to 99 is what ChineseNumber writes.
local TEST_MOST = 99

-- 1 to 99 in Chinese numerals: 7 is 七, 12 is 十二, 30 is 三十.
function ns.ChineseNumber(n)
    local digits = { "一", "二", "三", "四", "五", "六", "七", "八", "九" }
    local tens, ones = math.floor(n / 10), n % 10
    return (tens > 1 and digits[tens] or "") .. (tens > 0 and "十" or "") .. (digits[ones] or "")
end

local function ShowHelp()
    ns.Print("/tr (or /nms)  open or close the box for typing a reply")
    ns.Print("/tr hello  translate English into Chinese and copy it, to paste into chat")
    ns.Print("/tr /g hello  the same, and open the chat box on guild (or /p, /s, /w Name, /2 ...)")
    ns.Print("/tr /name  explain your target's name in the overlay, or /tr /name 黑熊 for one you type")
    ns.Print("/tr /options  open the options (also under Options > AddOns)")
    ns.Print("/tr /window General  have the overlay copy that chat window, or /tr /window all for all chat")
    ns.Print("/tr /all  toggle showing lines without Chinese in the overlay too")
    ns.Print("/tr /test  say a Chinese test line to check the strip and overlay")
    ns.Print("/tr /test 10  send that many short test lines to the overlay only (at most " .. TEST_MOST .. ")")
    ns.Print("/tr /probe  report which client features this addon found")
    ns.Print("/tr /help  this list")
end

local commands = {
    options = function()
        if ns.OpenOptions then ns.OpenOptions() else ns.Print("No options panel on this client.") end
    end,
    -- Which chat window the overlay copies, as under "Lines sent to the overlay" in the
    -- options: a window's name, or "all" for all chat. The companion's picker copies
    -- this command for the player to paste.
    window = function(_, given)
        local wanted = strtrim(given or "")
        if wanted == "" then
            local source = ns.db.sourceWindow or ""
            ns.Print("The overlay gets " .. (source == "" and "all chat" or ("what the " .. source .. " window shows"))
                .. ". Change it with /tr /window <name>, or /tr /window all.")
            return
        end
        if strlower(wanted) == "all" then
            ns.db.sourceWindow = ""
            ns.Print("The overlay now gets all chat.")
            return
        end
        for _, name in ipairs(ns.ChatWindowNames()) do
            if strlower(name) == strlower(wanted) then
                ns.db.sourceWindow = name
                ns.Print("The overlay now gets what the " .. name .. " window shows.")
                return
            end
        end
        ns.Print("No chat window named " .. wanted .. ". There are: " .. table.concat(ns.ChatWindowNames(), ", ") .. ".")
    end,
    all = function()
        ns.db.allLines = not ns.db.allLines
        ns.Print("The overlay shows " .. (ns.db.allLines and "all chat lines." or "Chinese lines only."))
    end,
    -- Explains a player's name in the overlay: the one given, or the target's.
    name = function(_, given)
        local name = given ~= "" and given or (UnitName and UnitName("target"))
        if ns.IsSecret(name) or type(name) ~= "string" or name == "" then
            ns.Print("Whose name? Target a player, or type one: /tr /name 黑熊")
            return
        end
        ns.ExplainName(name)
    end,
    -- With a count, sends that many short lines, each a number in Chinese,
    -- straight to the overlay: enough different lines to see how the
    -- translator copes with a burst. Nothing is said in chat: the game only
    -- lets an addon talk in /say right on a key press, not from a timer. The
    -- companion never reuses an earlier translation for a line marked
    -- "(Translate test)".
    test = function(count)
        if count then
            count = math.max(1, math.min(count, TEST_MOST))
            for i = 1, count do
                ns.SendTestLine(ns.ChineseNumber(i) .. " (Translate test)")
            end
            ns.Print("Sent " .. count .. " test lines to the overlay. They are not said in chat.")
            return
        end
        -- Mainline moved SendChatMessage into C_ChatInfo; probe both.
        local send = (C_ChatInfo and C_ChatInfo.SendChatMessage) or SendChatMessage
        if not send then
            ns.Print("No SendChatMessage on this client. Type /s 需要奶妈 by hand.")
            return
        end
        send("大家好，今天一起打副本嗎？ (Translate test)", "SAY")
        ns.Print("Sent a test line in /say. The companion overlay should show it in English shortly.")
    end,
    probe = function()
        local _, _, _, toc = GetBuildInfo()
        ns.Print("Interface " .. tostring(toc))
        ns.Print("issecretvalue: " .. tostring(issecretvalue ~= nil))
        ns.Print("Chat filter API: " .. tostring(ns.filterSource))
        ns.Print("Chat window positions: " .. tostring(ns.layoutSource))
        ns.Print("Options panel: " .. tostring(ns.optionsSource))
        ns.Print("Lines sent: " .. ((ns.db.sourceWindow or "") == "" and "all chat" or ("what the " .. ns.db.sourceWindow .. " window shows")))
        ns.Print("Chat window copying: " .. tostring(ns.mirrorSource))
        ns.Print("Link clicks: " .. tostring(ns.linkSource))
        ns.Print("Overlay scrolling: " .. tostring(ns.wheelSource))
        ns.Print("Chat window buttons: " .. tostring(ns.buttonSource))
        local font = ChatFrame1 and ChatFrame1.GetFont and ChatFrame1:GetFont()
        ns.Print("Chat font: " .. tostring(font))
        local w, h = GetPhysicalScreenSize()
        ns.Print("Physical screen: " .. tostring(w) .. "x" .. tostring(h))
        ns.Print("SendChatMessage: " .. ((C_ChatInfo and C_ChatInfo.SendChatMessage) and "C_ChatInfo" or (SendChatMessage and "global" or "none")))
    end,
    help = ShowHelp,
}

SLASH_TRANSLATE1 = "/tr"
SLASH_TRANSLATE2 = "/translate"
SLASH_TRANSLATE3 = "/nms"
-- "/tr" alone is the Translate button. A command is a word after a slash
-- ("/tr /test 5"), so no English sentence can be taken for one; a slash word
-- that is no command is a chat target ("/tr /g hello").
SlashCmdList.TRANSLATE = function(input)
    local text = strtrim(input or "")
    if text == "" then
        ns.SendCommand("reply")
        return
    end
    local word, argument = text:match("^/(%S+)%s*(.-)$")
    local fn = word and commands[strlower(word)]
    if fn then
        fn(tonumber(argument), argument)
    else
        ns.Reply(text)
    end
end
