local addonName, ns = ...

local CHAT_EVENTS = {
    "CHAT_MSG_SAY", "CHAT_MSG_YELL", "CHAT_MSG_WHISPER",
    "CHAT_MSG_PARTY", "CHAT_MSG_PARTY_LEADER",
    "CHAT_MSG_RAID", "CHAT_MSG_RAID_LEADER", "CHAT_MSG_RAID_WARNING",
    "CHAT_MSG_INSTANCE_CHAT", "CHAT_MSG_INSTANCE_CHAT_LEADER",
    "CHAT_MSG_GUILD", "CHAT_MSG_OFFICER", "CHAT_MSG_CHANNEL",
}

-- Lines that are not player chat: the game's own messages, emotes, creatures
-- and your own whispers.
local INFO_EVENTS = {
    "CHAT_MSG_SYSTEM", "CHAT_MSG_EMOTE", "CHAT_MSG_TEXT_EMOTE", "CHAT_MSG_WHISPER_INFORM",
    "CHAT_MSG_MONSTER_SAY", "CHAT_MSG_MONSTER_YELL", "CHAT_MSG_MONSTER_WHISPER", "CHAT_MSG_MONSTER_PARTY",
    "CHAT_MSG_LOOT", "CHAT_MSG_MONEY", "CHAT_MSG_CURRENCY", "CHAT_MSG_SKILL",
    "CHAT_MSG_COMBAT_XP_GAIN", "CHAT_MSG_COMBAT_HONOR_GAIN", "CHAT_MSG_COMBAT_FACTION_CHANGE",
    "CHAT_MSG_COMBAT_MISC_INFO", "CHAT_MSG_TRADESKILLS", "CHAT_MSG_OPENING", "CHAT_MSG_PET_INFO",
    "CHAT_MSG_BG_SYSTEM_NEUTRAL", "CHAT_MSG_BG_SYSTEM_ALLIANCE", "CHAT_MSG_BG_SYSTEM_HORDE",
}

-- CJK Unified Ideographs (U+4E00..U+9FFF) start with UTF-8 lead bytes E4..E9.
local function HasChinese(text)
    return text:find("[\228-\233][\128-\191][\128-\191]") ~= nil
end

-- A named color code (|cnNAME:) as an ordinary one, looked up among the
-- game's color objects. A name the game does not know stays as it is, and its
-- text then takes the color around it.
local function NamedColor(name)
    local color = _G[name]
    if type(color) ~= "table" or type(color.r) ~= "number" or type(color.g) ~= "number"
        or type(color.b) ~= "number" then
        return nil
    end
    return "|cff" .. ns.Hex(color.r, color.g, color.b)
end

-- What this client calls the thing a link points to, by the kind of link and
-- the number in it. A link in chat shows the name in its sender's language; the
-- number is the same for everyone, so the game can give the name in ours. Nil
-- for a thing the game has not loaded yet: asking makes it load, and
-- `unnamed` then tells SendChat to try the line again shortly.
local unnamed = false
local OWN_NAME = {
    item = function(id)
        return C_Item and C_Item.GetItemNameByID and C_Item.GetItemNameByID(id) or GetItemInfo and (GetItemInfo(id))
    end,
    quest = function(id)
        local log = C_QuestLog or {}
        if log.RequestLoadQuestByID then log.RequestLoadQuestByID(id) end
        return log.GetTitleForQuestID and log.GetTitleForQuestID(id) or log.GetQuestInfo and (log.GetQuestInfo(id))
    end,
    spell = function(id)
        return C_Spell and C_Spell.GetSpellName and C_Spell.GetSpellName(id) or GetSpellInfo and (GetSpellInfo(id))
    end,
    achievement = function(id)
        return GetAchievementInfo and select(2, GetAchievementInfo(id))
    end,
}
OWN_NAME.enchant = OWN_NAME.spell

-- A link's text for the overlay: a Chinese name gives way to this client's
-- name for the thing, inside the brackets and after a quest's level
-- ("[[31] 名]"). The text stays as it is when the game has no name to give.
local function LinkText(kind, id, text)
    local lookup = OWN_NAME[kind]
    if not lookup or not HasChinese(text) then return text end
    local ok, name = pcall(lookup, tonumber(id))
    if not ok or not name or ns.IsSecret(name) or type(name) ~= "string" or name == "" then
        unnamed = true
        return text
    end
    return text:match("^[%[%]%d%s+]*") .. name .. text:match("[%]%s]*$")
end

-- A line as the overlay draws it: links become their text, icons go, and the
-- color codes (|cAARRGGBB, |r, and |cnNAME: for unknown names) stay for the
-- companion to apply.
local function Markup(text)
    text = text:gsub("|cn([^:|]*):", NamedColor)
    text = text:gsub("|H(%a+):(%d+)[^|]*|h(.-)|h", LinkText)
    text = text:gsub("|H.-|h(.-)|h", "%1")
    text = text:gsub("|T.-|t", "")
    text = text:gsub("|A.-|a", "")
    text = text:gsub("|K.-|k", "?")
    text = text:gsub("|n", " ")
    -- Tabs and newlines separate fields and lines on the strip. Not "%c",
    -- which in some locales also matches bytes inside Chinese characters.
    text = text:gsub("[%z\1-\31]", " ")
    return text
end

-- Length of the color code that starts at byte i, or nil if none does.
local function CodeLength(markup, i)
    if markup:find("^|c%x%x%x%x%x%x%x%x", i) then return 10 end
    if markup:find("^|r", i) then return 2 end
    local _, last = markup:find("^|cn[^:|]*:", i)
    return last and last - i + 1
end

-- Markup without its color codes: the text as read.
local function Plain(markup)
    local parts, i = {}, 1
    while i <= #markup do
        local code = CodeLength(markup, i)
        if code then
            i = i + code
        else
            local nextBar = markup:find("|", i + 1, true) or (#markup + 1)
            parts[#parts + 1] = markup:sub(i, nextBar - 1)
            i = nextBar
        end
    end
    return table.concat(parts)
end

-- Where in markup the text starts that follows `count` bytes of plain text.
-- Color codes at the cut stay with the part before it.
local function MarkupIndex(markup, count)
    local i, seen = 1, 0
    while i <= #markup do
        local code = CodeLength(markup, i)
        if code then
            i = i + code
        elseif seen >= count then
            break
        else
            i = i + 1
            seen = seen + 1
        end
    end
    return i
end

function ns.Hex(r, g, b)
    local function Byte(v)
        v = type(v) == "number" and v or 1
        return math.floor(math.max(0, math.min(1, v)) * 255 + 0.5)
    end
    return string.format("%02x%02x%02x", Byte(r), Byte(g), Byte(b))
end

-- One line for the overlay: "<kind>\t<prefix>\t<body>\t<rrggbb>\t<speaker>\t<number>".
-- Prefix and body are markup and together make the line as shown in chat, in
-- the given color; the speaker is the player's name as shown, or empty. Kind
-- 1 asks for the body to be translated; with 0 the line is shown as it is.
-- T, E and N are a line the player clicked a link on: to translate it again,
-- to explain it word by word, or to explain the speaker's name. They go ahead
-- of everything waiting, here and in the companion, and have no number. A
-- body too long for the strip is cut, so the fields after it always arrive.
local function Line(kind, prefix, body, color, speaker, number)
    local fields = { kind, prefix, body, color, speaker or "", number }
    local others = #table.concat(fields, "\t") - #body
    fields[3] = ns.Truncate(body, math.max(ns.STRIP.maxText - others, 0))
    return table.concat(fields, "\t")
end

-- Sends a line the player asked for by a link or a command.
local function Ask(kind, prefix, body, color, speaker)
    local line = Line(kind, prefix, body, color, speaker)
    ns.SendToStrip(line, line)
end

-- The number tells the lines of the chat apart and puts them in order. The
-- strip shows the last of them again and again while nothing new waits, for a
-- companion that was not looking the first time (the game was covered, or the
-- companion not running); it drops the numbers it has seen. Numbers start at
-- the time of login, so they keep rising from one login to the next.
local number = time() * 1000

local NAME_TRIES = 5 -- how many times a line is built again for the names of its links

-- Sends a line of the chat. `build` returns its kind (0 or 1), prefix, body,
-- color and speaker, or nothing for a line that is not sent. Where the game
-- could not yet name a link, the line goes out with the sender's Chinese name
-- for it, which the companion translates word for word. The game loads the
-- name meanwhile, so the line is built again the next few times the strip
-- repeats it; the companion puts a line that reads differently under the same
-- number in the earlier one's place.
local function SendChat(build)
    unnamed = false
    local kind, prefix, body, color, speaker = build()
    if not kind then return end
    number = number + 1
    local own = string.format("%.0f", number)
    local line = Line(kind, prefix, body, color, speaker, own)
    local tries = unnamed and NAME_TRIES or 0
    ns.SendToStrip(line)
    ns.RepeatOnStrip(function()
        if tries > 0 then
            unnamed = false
            local kind, prefix, body, color, speaker = build()
            if kind then line = Line(kind, prefix, body, color, speaker, own) end
            tries = unnamed and tries - 1 or 0
        end
        return line
    end)
end

ns.Markup, ns.Plain, ns.MarkupIndex = Markup, Plain, MarkupIndex

-- The chat lines already handled, by the game's lineID: the message's plain
-- text if it has Chinese, false if not. Every chat window reports a line, so
-- this is how a line is sent once, and how the message is found in the line a
-- window words around it. Forgets the oldest beyond KEEP.
local KEEP = 300
local handled, order, first = {}, {}, 1

local function Remember(lineID, value)
    handled[lineID] = value
    order[#order + 1] = lineID
    if #order - first >= KEEP then
        handled[order[first]] = nil
        order[first] = nil
        first = first + 1
    end
end

-- How the game words each kind of line around the speaker's name.
local AROUND_NAME = {
    CHAT_MSG_SAY = { "[", "] says: " }, CHAT_MSG_YELL = { "[", "] yells: " },
    CHAT_MSG_WHISPER = { "[", "] whispers: " }, CHAT_MSG_WHISPER_INFORM = { "To [", "]: " },
    CHAT_MSG_PARTY = { "[Party] [", "]: " }, CHAT_MSG_PARTY_LEADER = { "[Party Leader] [", "]: " },
    CHAT_MSG_RAID = { "[Raid] [", "]: " }, CHAT_MSG_RAID_LEADER = { "[Raid Leader] [", "]: " },
    CHAT_MSG_RAID_WARNING = { "[Raid Warning] [", "]: " },
    CHAT_MSG_INSTANCE_CHAT = { "[Instance] [", "]: " }, CHAT_MSG_INSTANCE_CHAT_LEADER = { "[Instance Leader] [", "]: " },
    CHAT_MSG_GUILD = { "[Guild] [", "]: " }, CHAT_MSG_OFFICER = { "[Officer] [", "]: " },
    CHAT_MSG_EMOTE = { "", " " },
    CHAT_MSG_MONSTER_SAY = { "", " says: " }, CHAT_MSG_MONSTER_YELL = { "", " yells: " },
    CHAT_MSG_MONSTER_WHISPER = { "", " whispers: " }, CHAT_MSG_MONSTER_PARTY = { "[Party] ", ": " },
}

-- The color the game gives a kind of line, as the player set it up.
local function LineColor(event, channelIndex)
    local kind = event:sub(10) -- after "CHAT_MSG_"
    local info = ChatTypeInfo and ChatTypeInfo[kind]
    if kind == "CHANNEL" and ChatTypeInfo and type(channelIndex) == "number" then
        info = ChatTypeInfo["CHANNEL" .. channelIndex] or info
    end
    if not info then return "ffffff", nil end
    return ns.Hex(info.r, info.g, info.b), info
end

-- The name in its class color, when the player turned that on for this kind of line.
local function NameMarkup(name, guid, info)
    if info and info.colorNameByClass and type(guid) == "string" and GetPlayerInfoByGUID then
        local _, class = GetPlayerInfoByGUID(guid)
        local color = class and RAID_CLASS_COLORS and RAID_CLASS_COLORS[class]
        if color and color.colorStr then return "|c" .. color.colorStr .. name .. "|r" end
    end
    return name
end

-- The names on the tabs of the game's chat windows, in window order.
function ns.ChatWindowNames()
    local names = {}
    if not FCF_GetChatWindowInfo then return names end
    for i = 1, NUM_CHAT_WINDOWS or 10 do
        local frame = _G["ChatFrame" .. i]
        local name = FCF_GetChatWindowInfo(i)
        if frame and type(name) == "string" and name ~= "" and (frame:IsShown() or frame.isDocked) then
            names[#names + 1] = name
        end
    end
    return names
end

local function WindowNamed(wanted)
    if not FCF_GetChatWindowInfo then return nil end
    for i = 1, NUM_CHAT_WINDOWS or 10 do
        local frame = _G["ChatFrame" .. i]
        if frame and FCF_GetChatWindowInfo(i) == wanted then return frame end
    end
end

-- The chat window whose lines are copied to the overlay, if one is chosen in
-- the options and still exists. Without one, lines are sent as their chat
-- events arrive.
local function MirroredWindow()
    local wanted = ns.db.sourceWindow
    if type(wanted) ~= "string" or wanted == "" then return nil end
    return WindowNamed(wanted)
end

-- Takes a chat event's line once, and sends it to the pixel strip unless a
-- chat window is mirrored: then Mirror sends what that window shows, and the
-- line is only remembered for it. Lines without Chinese are sent only with
-- allLines on, so the overlay can show the whole conversation in order.
-- Lines that cannot be read (secret values) are left alone. Returns the
-- line's lineID and whether its message has Chinese, for its links.
-- Arguments follow the CHAT_MSG_* payload: the 4th is the channel's full
-- name, the 8th its number, the 11th the lineID, the 12th the sender's GUID.
local function Capture(event, msg, author, _, channel, _, _, _, channelIndex, _, _, lineID, guid)
    if ns.IsSecret(msg) or ns.IsSecret(author) or ns.IsSecret(channel) or ns.IsSecret(lineID)
        or ns.IsSecret(channelIndex) or ns.IsSecret(guid) then
        return
    end
    if type(msg) ~= "string" then return end
    if lineID ~= nil and handled[lineID] ~= nil then return lineID, handled[lineID] ~= false end
    local chinese = HasChinese(msg)
    if lineID ~= nil then Remember(lineID, chinese and Plain(Markup(msg))) end
    if MirroredWindow() or not (chinese or ns.db.allLines) then return lineID, chinese end

    local name = type(author) == "string" and author ~= "" and Ambiguate(author, "short") or nil
    local color, info = LineColor(event, channelIndex)
    local around = AROUND_NAME[event]
    local prefix = ""
    if event == "CHAT_MSG_CHANNEL" and name then
        local label = type(channel) == "string" and channel ~= "" and ("[" .. channel .. "] ") or ""
        prefix = label .. "[" .. NameMarkup(name, guid, info) .. "]: "
    elseif around and name then
        prefix = around[1] .. NameMarkup(name, guid, info) .. around[2]
    end
    SendChat(function() return chinese and "1" or "0", Markup(prefix), Markup(msg), color, name end)
    return lineID, chinese
end

-- Breaks a line, as a chat window words it, into the part before the message
-- and the message, both as markup, and finds the speaker's name as shown.
-- Without Chinese in the message there is nothing to translate: then the
-- first part is nil and the second the whole line.
local function Split(text)
    -- The speaker's link holds the name as shown, and the lineID of the
    -- message Capture remembered. The overlay has no use for our own links.
    local lineID, shown = text:match("|Hplayer:[^:|]*:(%d+)[^|]*|h(.-)|h")
    local speaker = shown and Plain(Markup(shown)):gsub("^%[(.*)%]$", "%1") or ""
    local message = lineID and handled[tonumber(lineID)]
    text = text:gsub(" *|Haddon:" .. addonName .. ":%a:%d+|h.-|h", "")

    local markup = Markup(text)
    local plain = Plain(markup)
    local prefix, body = "", markup
    if HasChinese(plain) then
        -- The message starts where its remembered text is found, or failing
        -- that (icons, other addons' edits, a line long past) after the first ": ".
        local at = message and plain:find(message, 1, true)
        if not at then
            local _, last = plain:find(": ", 1, true)
            at = last and last + 1
        end
        if at and at > 1 then
            local cut = MarkupIndex(markup, at - 1)
            prefix, body = markup:sub(1, cut - 1), markup:sub(cut)
        end
    end
    -- Without Chinese in the body, only the speaker's name is Chinese, or nothing is.
    if HasChinese(Plain(body)) then return prefix, body, speaker end
    return nil, markup, speaker
end

local function Mirror(frame, text, r, g, b)
    if not ns.db or frame ~= MirroredWindow() then return end
    if ns.IsSecret(text) or ns.IsSecret(r) or ns.IsSecret(g) or ns.IsSecret(b) then return end
    if type(text) ~= "string" then return end
    SendChat(function()
        local prefix, body, speaker = Split(text)
        if prefix then return "1", prefix, body, ns.Hex(r, g, b), speaker end
        if ns.db.allLines then return "0", "", body, ns.Hex(r, g, b), speaker end
    end)
end

-- The links after a line: [T] sends it to the overlay to be translated again
-- and [?] to be explained word by word, for a message with Chinese; [?N] has
-- the speaker's name explained, for a name with Chinese. A link names what it
-- asks for, as the kind of strip line to send, and the line.
local function Links(lineID, chineseMessage, chineseName)
    local function Link(kind, label)
        return "|Haddon:" .. addonName .. ":" .. kind .. ":" .. lineID .. "|h|cff66ccff" .. label .. "|r|h"
    end
    return (chineseMessage and (Link("T", "[T]") .. Link("E", "[?]")) or "") .. (chineseName and Link("N", "[?N]") or "")
end

-- Sends a line to the overlay when one of its links is clicked, read back from
-- the chat window it was clicked in, so any line still on show can be asked
-- for. The companion marks it as asked for and handles it first.
local function SendAgain(kind, lineID, clicked)
    local link = "|Haddon:" .. addonName .. ":%a:" .. lineID .. "|h"
    local windows = { clicked }
    for i = 1, NUM_CHAT_WINDOWS or 10 do windows[#windows + 1] = _G["ChatFrame" .. i] end
    for _, window in ipairs(windows) do
        local count = window.GetNumMessages and window:GetNumMessages()
        for i = 1, type(count) == "number" and count or 0 do
            local text, r, g, b = window:GetMessageInfo(i)
            if type(text) == "string" and not ns.IsSecret(text) and text:find(link) then
                local prefix, body, speaker = Split(text)
                if kind == "N" then
                    -- Only the name matters; the companion words the line itself.
                    Ask(kind, "", "", ns.Hex(r, g, b), speaker)
                elseif prefix then
                    Ask(kind, prefix, body, ns.Hex(r, g, b), speaker)
                end
                return
            end
        end
    end
    ns.Print("That line is no longer in a chat window, so it cannot be sent again.")
end

-- Asks the companion to explain a player's name, as a [?N] link does.
function ns.ExplainName(name)
    Ask("N", "", "", "ffffff", name)
end

-- Class colors as the game has them, for test lines on a client without RAID_CLASS_COLORS.
local CLASS_COLORS = {
    WARRIOR = "ffc69b6d", PALADIN = "fff48cba", HUNTER = "ffaad372", ROGUE = "fffff468", PRIEST = "ffffffff",
    SHAMAN = "ff0070dd", MAGE = "ff3fc7eb", WARLOCK = "ff8788ee", DRUID = "ffff7c0a",
}

-- Made-up chat for "/tr /test <count>": the kinds of line, speakers and words a Taiwanese
-- or mainland server has, so the overlay shows what real chat looks like: the channel's
-- color, class-colored names in Chinese, an item link, Traditional and Simplified.
local TEST_CHAT = {
    { "CHAT_MSG_PARTY", "奶媽小美", "PRIEST", "缺坦，來個戰士就開" },
    { "CHAT_MSG_GUILD", "冰霜法爺", "MAGE", "今晚團本幾點開？" },
    { "CHAT_MSG_SAY", "獵人阿傑", "HUNTER", "有人要組死亡礦坑嗎" },
    { "CHAT_MSG_WHISPER", "影舞者", "ROGUE", "密我，可以帶你" },
    { "CHAT_MSG_RAID", "狂暴戰神", "WARRIOR", "等我一下，马上到" },
    { "CHAT_MSG_CHANNEL", "德魯伊老王", "DRUID",
        "收 |cffa335ee|Hitem:19019::::::::60:::::|h[雷霆之怒，逐風者的祝福之劍]|h|r 價格私聊", "1. 綜合" },
    { "CHAT_MSG_YELL", "聖光小騎", "PALADIN", "這邊有稀有怪！快來！" },
    { "CHAT_MSG_PARTY", "薩滿大叔", "SHAMAN", "奶妈没蓝了，休息一下" },
    { "CHAT_MSG_GUILD", "暗影術士", "WARLOCK", "謝謝大家 3q" },
    { "CHAT_MSG_RAID", "奶媽小美", "PRIEST", "注意站位，別站火裡" },
}
ns.TEST_CHAT_SIZE = #TEST_CHAT

-- The `i`th made-up chat line for the overlay, as if someone had said it, worded and
-- colored the way the game words and colors its kind of line. The test mark has the
-- companion translate it afresh each time.
function ns.SendTestLine(i)
    local event, name, class, text, channel = unpack(TEST_CHAT[(i - 1) % #TEST_CHAT + 1])
    local color = LineColor(event, channel and 1)
    local classColor = RAID_CLASS_COLORS and RAID_CLASS_COLORS[class]
    local named = "|c" .. (classColor and classColor.colorStr or CLASS_COLORS[class]) .. name .. "|r"
    local prefix
    if channel then
        prefix = "[" .. channel .. "] [" .. named .. "]: "
    else
        local around = AROUND_NAME[event]
        prefix = around[1] .. named .. around[2]
    end
    SendChat(function() return "1", Markup(prefix), Markup(text .. " (Translate test)"), color, name end)
end

-- Runs Mirror for tests, and behind the hook where an error must not reach the chat window.
function ns.MirrorLine(frame, text, r, g, b)
    local ok, problem = pcall(Mirror, frame, text, r, g, b)
    if not ok then ns.mirrorSource = "failed: " .. tostring(problem) end
end

-- Runs before a chat window words and adds the line, so Mirror finds it
-- remembered. Adds the links to a line with Chinese in its message or in its
-- speaker's name.
-- The glossary (Glossary.lua, written by the companion): WoW terms and whole chat lines
-- with their English, shown after them in the chat window. Each note is a link of kind
-- G that does nothing when clicked: the addon's links are taken out of a line before it
-- is copied to the overlay, so the notes never reach the translator.
local GLOSS_COLOR = "ff9d9d9d"
local MOST_NOTES = 6 -- notes on one line at most, so a line stays readable

-- Bytes in the UTF-8 character that starts with `byte`.
local function CharLength(byte)
    if not byte or byte < 192 then return 1 elseif byte < 224 then return 2 elseif byte < 240 then return 3 end
    return 4
end

local termIndex -- first character of a term, lower-cased, to its terms, longest first

local function TermIndex()
    if termIndex then return termIndex end
    termIndex = {}
    for zh, en in pairs(ns.GLOSSARY_TERMS or {}) do
        local key = zh:lower()
        local first = key:sub(1, CharLength(key:byte(1)))
        termIndex[first] = termIndex[first] or {}
        table.insert(termIndex[first], { key, en })
    end
    for _, terms in pairs(termIndex) do
        table.sort(terms, function(a, b) return #a[1] > #b[1] end)
    end
    return termIndex
end

local function Note(english, lineID)
    return "|Haddon:" .. addonName .. ":G:" .. lineID .. "|h|c" .. GLOSS_COLOR .. "(" .. english .. ")|r|h"
end

local function IsWordByte(byte)
    return byte and ((byte >= 48 and byte <= 57) or (byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122))
end

-- Notes after the terms in a run of plain text. Chinese has no spaces, so a term may
-- start at any character; the longest one there wins. A term that starts or ends with
-- a Latin letter or digit must not carry on into another (DD in "adds").
local function GlossPlain(text, lineID, budget)
    local index, lower, out, i, noted = TermIndex(), text:lower(), {}, 1, {}
    while i <= #text do
        local found
        if budget.left > 0 then
            for _, term in ipairs(index[lower:sub(i, i + CharLength(lower:byte(i)) - 1)] or {}) do
                local key = term[1]
                if lower:sub(i, i + #key - 1) == key
                    and not (IsWordByte(key:byte(1)) and IsWordByte(lower:byte(i - 1)))
                    and not (IsWordByte(key:byte(-1)) and IsWordByte(lower:byte(i + #key))) then
                    found = term
                    break
                end
            end
        end
        if found then
            local written = text:sub(i, i + #found[1] - 1)
            out[#out + 1] = written
            if not noted[found[2]] then
                noted[found[2]] = true
                budget.left = budget.left - 1
                out[#out + 1] = Note(found[2], lineID)
            end
            i = i + #found[1]
        else
            -- One whole UTF-8 character.
            local length = CharLength(text:byte(i))
            out[#out + 1] = text:sub(i, i + length - 1)
            i = i + length
        end
    end
    return table.concat(out)
end

-- The message with the glossary's notes: the English of the whole line, if it was said
-- often enough for the companion to put it in the glossary, or else notes after the WoW
-- terms in it. Links, colors and icons are left as they are.
function ns.Gloss(msg, lineID)
    local key = Plain(Markup(msg)):gsub("%s+", " "):gsub("^ ", ""):gsub(" $", "")
    local line = ns.GLOSSARY_LINES and ns.GLOSSARY_LINES[key]
    if line then return msg .. " " .. Note(line, lineID) end
    local out, i, budget = {}, 1, { left = MOST_NOTES }
    while i <= #msg do
        local bar = msg:find("|", i, true)
        local plain = msg:sub(i, (bar or #msg + 1) - 1)
        if plain ~= "" then out[#out + 1] = GlossPlain(plain, lineID, budget) end
        if not bar then break end
        -- An escape sequence, copied as it is: a link to its closing |h, a color, an icon.
        local escape = msg:match("^|H.-|h.-|h", bar) or msg:match("^|c%x%x%x%x%x%x%x%x", bar)
            or msg:match("^|T.-|t", bar) or msg:match("^|A.-|a", bar) or msg:match("^|K.-|k", bar)
            or msg:sub(bar, bar + 1)
        out[#out + 1] = escape
        i = bar + #escape
    end
    return table.concat(out)
end

-- How much of the glossary the game loaded, in words. Glossary.lua is missing until the
-- game client is started anew after the addon gained it: /reload does not read new files.
function ns.GlossaryLoaded()
    if not ns.GLOSSARY_TERMS then
        return "Glossary: not loaded. Quit the game fully and start it again: it only finds new addon files at start."
    end
    local terms, lines = 0, 0
    for _ in pairs(ns.GLOSSARY_TERMS) do terms = terms + 1 end
    for _ in pairs(ns.GLOSSARY_LINES or {}) do lines = lines + 1 end
    return "Glossary: " .. terms .. " terms and " .. lines .. " remembered lines, notes "
        .. (ns.db.glossary and "on" or "off") .. "."
end

local function Filter(_, event, msg, author, ...)
    local lineID, chinese = Capture(event, msg, author, ...)
    if not lineID or type(msg) ~= "string" or ns.IsSecret(msg) then return false end
    local named = type(author) == "string" and not ns.IsSecret(author) and HasChinese(author)
    local out = msg
    if chinese and ns.db.glossary then out = ns.Gloss(msg, lineID) end
    if ns.db.showLinks and (chinese or named) then out = out .. " " .. Links(lineID, chinese, named) end
    if out == msg then return false end
    return false, out, author, ...
end

function ns.InitChat()
    -- Mainline moved some ChatFrame_ globals into ChatFrameUtil; probe both.
    local addFilter = ChatFrame_AddMessageEventFilter
    ns.filterSource = "ChatFrame_AddMessageEventFilter"
    if not addFilter and ChatFrameUtil and ChatFrameUtil.AddMessageEventFilter then
        addFilter = ChatFrameUtil.AddMessageEventFilter
        ns.filterSource = "ChatFrameUtil.AddMessageEventFilter"
    end
    if not addFilter then
        ns.filterSource = "none"
        ns.Print("No chat filter API found. A copied chat window may translate the speaker's name with the line.")
    else
        for _, event in ipairs(CHAT_EVENTS) do addFilter(event, Filter) end
    end

    -- Also capture lines on channels no chat window shows. Capture skips
    -- lines the filter already took.
    for _, event in ipairs(CHAT_EVENTS) do
        ns.On(event, function(...) Capture(event, ...) end)
    end
    for _, event in ipairs(INFO_EVENTS) do
        ns.On(event, function(...) Capture(event, ...) end)
    end

    -- Every chat window is hooked once; Mirror acts only on the chosen one.
    local hooked = 0
    for i = 1, NUM_CHAT_WINDOWS or 10 do
        local frame = _G["ChatFrame" .. i]
        if frame and type(frame.AddMessage) == "function" and hooksecurefunc then
            hooksecurefunc(frame, "AddMessage", ns.MirrorLine)
            hooked = hooked + 1
        end
    end
    ns.mirrorSource = hooked .. " chat windows hooked"

    -- Clicks on |Haddon:...| links arrive through EventRegistry.
    if EventRegistry and EventRegistry.RegisterCallback then
        EventRegistry:RegisterCallback("SetItemRef", function(_, link, _, _, window)
            local name, kind, lineID = tostring(link):match("^addon:([^:]+):([TEN]):(%d+)")
            if name == addonName then SendAgain(kind, lineID, type(window) == "table" and window or nil) end
        end, ns)
        ns.linkSource = "EventRegistry SetItemRef"
    else
        ns.linkSource = "none: the links after chat lines do nothing on this client"
    end
end
