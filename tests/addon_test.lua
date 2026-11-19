-- Tests for the addon, run outside the game with Lua 5.1:
--     lua5.1 tests/addon_test.lua        (from the repository root)
-- The game's functions the addon calls are replaced by small stand-ins, and what the
-- addon would draw on the pixel strip is collected as text.

local ADDON = "addon/Translate/"

-- An object whose every method does nothing and returns another such object, standing
-- in for frames, textures and buttons.
local function Dummy()
    return setmetatable({}, { __index = function() return function() return Dummy() end end })
end

-- A chat window: AddMessage and HookScript are real enough to hook, the rest is a dummy.
local function ChatWindow()
    local window = Dummy()
    window.added, window.scripts, window.isDocked = {}, {}, true
    window.AddMessage = function(self, text, r, g, b) self.added[#self.added + 1] = { text, r, g, b } end
    window.GetNumMessages = function(self) return #self.added end
    window.GetMessageInfo = function(self, i) return unpack(self.added[i]) end
    window.HookScript = function(self, script, fn) self.scripts[script] = fn end
    return window
end

local printed, said, opened, timers = {}, {}, {}, {}
strlower, strtrim = string.lower, function(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end
function CreateFrame() return Dummy() end
UIParent = Dummy()
DEFAULT_CHAT_FRAME = { AddMessage = function(_, text) printed[#printed + 1] = text end }
SlashCmdList = {}
C_Timer = { After = function(_, fn) timers[#timers + 1] = fn end, NewTicker = function() return Dummy() end }
function time() return 1700000000 end
function GetPhysicalScreenSize() return 1920, 1080 end
function GetCVarBool() return true end
function hooksecurefunc(object, name, hook)
    local original = object[name]
    object[name] = function(...) original(...); hook(...) end
end
function Ambiguate(name) return name end
function SendChatMessage(text, kind) said[#said + 1] = kind .. " " .. text end
function ChatFrame_OpenChat(text) opened[#opened + 1] = text end
local filters = {}
function ChatFrame_AddMessageEventFilter(event, fn) filters[event] = fn end
NUM_CHAT_WINDOWS = 2
ChatFrame1, ChatFrame2 = ChatWindow(), ChatWindow()
local tabNames = { "General", "Pick" }
function FCF_GetChatWindowInfo(i) return tabNames[i], 14 end
ChatTypeInfo = {
    SAY = { r = 1, g = 1, b = 1 }, GUILD = { r = 0.25, g = 1, b = 0.25, colorNameByClass = true },
    SYSTEM = { r = 1, g = 1, b = 0 }, CHANNEL = { r = 1, g = 0.75, b = 0.75 }, CHANNEL2 = { r = 0, g = 0.5, b = 1 },
}
RAID_CLASS_COLORS = { MAGE = { colorStr = "ff3fc7eb" } }
local itemNames = { [4338] = "Mageweave Cloth" }
function GetItemInfo(id) return itemNames[id] end
C_QuestLog = { GetTitleForQuestID = function(id) return id == 681 and "Stromgarde Badges" or nil end }
function GetPlayerInfoByGUID() return "Mage", "MAGE" end
RED_FONT_COLOR = { r = 1, g = 0.125, b = 0.125 }
local clickLink
EventRegistry = {
    RegisterCallback = function(_, _, fn) clickLink = function(link, window) fn(nil, link, "[T]", "LeftButton", window) end end,
}

-- Load the addon as the game does, in the order of Translate.toc, and log in.
local ns = {}
for _, file in ipairs({ "Core.lua", "Glossary.lua", "Strip.lua", "Chat.lua", "Layout.lua", "Buttons.lua", "Options.lua" }) do
    assert(loadfile(ADDON .. file))("Translate", ns)
end
ns.handlers.ADDON_LOADED("Translate")
ns.handlers.PLAYER_LOGIN()

-- The strip shows one frame at a time; `Frames` plays out the waiting ones and returns
-- the text of each frame shown since the last call, tabs written as " | ". The numbers
-- that end chat lines are left out, and no line is shown again, except where that is
-- what is tested.
ns.db.repeatLines = 0
local frames, numbered = {}, false
local encode = ns.EncodeFrame
ns.EncodeFrame = function(seq, text)
    if not numbered then text = (text .. "\n"):gsub("\t17%d+\n", "\n"):sub(1, -2) end
    frames[#frames + 1] = text:gsub("\t", " | ")
    return encode(seq, text)
end
local function Frames()
    while #timers > 0 do table.remove(timers, 1)() end
    local shown = frames
    frames = {}
    return shown
end
Frames() -- the layout sent at login

local failures = 0
local function Check(name, got, want)
    if type(got) == "table" then got = table.concat(got, "\n") end
    if type(want) == "table" then want = table.concat(want, "\n") end
    if got ~= want then
        failures = failures + 1
        print("FAILED: " .. name .. "\n  got:  " .. tostring(got):gsub("\n", "\n        ") .. "\n  want: " .. tostring(want):gsub("\n", "\n        "))
    end
end

-- A chat event as the game delivers it: through every chat window's filter, then to
-- the addon's own handler. Returns the lineID and the message as the filter left it.
local lineID = 0
local function Chat(event, text, channel, channelIndex)
    lineID = lineID + 1
    local args = { text, "Bob", "", channel or "", "", "", 0, channelIndex or 0, "", 0, lineID, "guid-1" }
    filters[event](nil, event, unpack(args))
    local _, shown = filters[event](nil, event, unpack(args))
    ns.handlers[event](unpack(args))
    return lineID, shown or text
end

-- Chat events, with no chat window copied ------------------------------------------

Chat("CHAT_MSG_SAY", "hello |cffff0000red|r")
Chat("CHAT_MSG_GUILD", "副本缺補")
Chat("CHAT_MSG_CHANNEL", "wts |cffa335ee|Hitem:1|h[Sword]|h|r |TInterface\\icon:0|t", "2. Trade - City", 2)
ns.handlers.CHAT_MSG_SYSTEM("Bob has come online.", "")
ns.handlers.CHAT_MSG_MONSTER_YELL("You will die!", "Ragnaros")
Check("each event's line is sent once, worded and colored as the game does", Frames(), {
    "0 | [Bob] says:  | hello |cffff0000red|r | ffffff | Bob",
    "1 | [Guild] [|cff3fc7ebBob|r]:  | 副本缺補 | 40ff40 | Bob\n"
        .. "0 | [2. Trade - City] [Bob]:  | wts |cffa335ee[Sword]|r  | 0080ff | Bob\n"
        .. "0 |  | Bob has come online. | ffff00 | \n"
        .. "0 | Ragnaros yells:  | You will die! | ffffff | Ragnaros",
})

TranslateDB.allLines = false
Chat("CHAT_MSG_SAY", "plain")
Chat("CHAT_MSG_SAY", "缺坦")
Check("without allLines only Chinese lines are sent", Frames(), { "1 | [Bob] says:  | 缺坦 | ffffff | Bob" })
TranslateDB.allLines = true

-- A copied chat window ----------------------------------------------------------------

-- [T] links ------------------------------------------------------------------------------

TranslateDB.glossary = false -- its notes are checked on their own below
local asked, withLink = Chat("CHAT_MSG_GUILD", "缺補")
local _, plainLine = Chat("CHAT_MSG_SAY", "no link here")
Check("a Chinese line gets its two links", withLink,
    "缺補 |Haddon:Translate:T:" .. asked .. "|h|cff66ccff[T]|r|h|Haddon:Translate:E:" .. asked .. "|h|cff66ccff[?]|r|h")
Check("other lines get none", plainLine, "no link here")
Frames()
-- The line as a chat window shows it, long after the addon forgot the message itself.
ChatFrame1:AddMessage("|Hplayer:Bob:424242:GUILD|h[|cff3fc7ebBob|r]|h: " .. withLink, 0.25, 1, 0.25)
ChatFrame1:AddMessage("|Hplayer:小明:424243:SAY|h[|cff3fc7eb小明|r]|h says: hi |Haddon:Translate:N:77|h[?N]|h", 1, 1, 1)
ns.SendToStrip("0\t\twaiting 1")
ns.SendToStrip("0\t\twaiting 2")
clickLink("addon:Translate:T:" .. asked, ChatFrame1)
clickLink("addon:Translate:E:" .. asked, ChatFrame1)
clickLink("addon:Other:T:" .. asked, ChatFrame1)
clickLink("item:1", ChatFrame1)
clickLink("addon:Translate:N:77")
Check("a click sends the line, read from the window, ahead of what is waiting", Frames(), {
    "0 |  | waiting 1",
    "N |  |  | ffffff | 小明\nE | [|cff3fc7ebBob|r]:  | 缺補 | 40ff40 | Bob\nT | [|cff3fc7ebBob|r]:  | 缺補 | 40ff40 | Bob\n0 |  | waiting 2",
})
local _, named = filters.CHAT_MSG_SAY(nil, "CHAT_MSG_SAY", "hello", "小明", "", "", "", "", 0, 0, "", 0, 555555, "guid-2")
Check("an English line from a Chinese name gets only the name's link", named, "hello |Haddon:Translate:N:555555|h|cff66ccff[?N]|r|h")
Frames()
printed = {}
clickLink("addon:Translate:T:99999", ChatFrame1)
Check("a line no window shows any more says so", #printed, 1)
TranslateDB.showLinks = false
local _, unlinked = Chat("CHAT_MSG_GUILD", "缺坦")
Check("no links with the option off", unlinked, "缺坦")
TranslateDB.glossary = true
local _, glossed = Chat("CHAT_MSG_GUILD", "缺坦")
Check("with the glossary on, a term gets its note and no links with them off", glossed,
    "缺坦|Haddon:Translate:G:" .. tostring(glossed:match("G:(%d+)")) .. "|h|cff9d9d9d(need a tank)|r|h")
TranslateDB.showLinks = true
Frames()

-- A copied chat window ----------------------------------------------------------------

TranslateDB.sourceWindow = "Pick"
local id, linked = Chat("CHAT_MSG_GUILD", "來打副本 |cffa335ee|Hitem:1|h[劍]|h|r")
Check("events send nothing while a window is copied", Frames(), {})

local function Shown(text, r, g, b) ChatFrame2:AddMessage(text, r or 1, g or 1, b or 1) end
Shown("|Hchannel:GUILD|h[Guild]|h |Hplayer:Bob:" .. id .. ":GUILD|h[|cff3fc7ebBob|r]|h: " .. linked, 0.25, 1, 0.25)
ChatFrame1:AddMessage("not the copied window", 1, 1, 1)
Shown("|cff33ff99DBM|r: Pull in |cffffff005|r")
Shown("12:30 |Hplayer:小明:999:SAY|h[小明]|h says: hello there")
Shown("|Hplayer:小明:998:SAY|h[小明]|h says: {star} 缺坦")
Shown("你獲得了物品", 1, 1, 0)
Shown("|cnRED_FONT_COLOR:Low health|r and |cnNO_SUCH_COLOR:this|r")
Check("the copied window's lines, split where the message starts", Frames(), {
    "1 | [Guild] [|cff3fc7ebBob|r]:  | 來打副本 |cffa335ee[劍]|r | 40ff40 | Bob",
    "0 |  | |cff33ff99DBM|r: Pull in |cffffff005|r | ffffff | \n"
        .. "0 |  | 12:30 [小明] says: hello there | ffffff | 小明\n"
        .. "1 | [小明] says:  | {star} 缺坦 | ffffff | 小明\n"
        .. "1 |  | 你獲得了物品 | ffff00 | \n"
        .. "0 |  | |cffff2020Low health|r and |cnNO_SUCH_COLOR:this|r | ffffff | ",
})

Shown(string.rep("長", 200))
local long = Frames()[1]
Check("a line too long for a frame keeps its color at the end", long:sub(-12), " | ffffff | ")
Check("and fits the frame", #long <= ns.STRIP.maxText + 8, true) -- " | " is 2 longer than a tab, 4 times
TranslateDB.sourceWindow = ""

-- Markup helpers -----------------------------------------------------------------------

Check("Plain drops color codes only", ns.Plain("a|cff112233b|cnRED_FONT_COLOR:c|r|rd|x"), "abcd|x")
Check("MarkupIndex keeps codes at the cut with the part before", ns.MarkupIndex("|cff112233ab|rcd", 2), 15)

-- Links ---------------------------------------------------------------------------------

Chat("CHAT_MSG_SAY", "收|cffffffff|Hitem:4338::::::::60:::::|h[魔紋布]|h|r 和 |cffffff00|Hquest:681:37|h[[37] 激流堡符印]|h|r")
Chat("CHAT_MSG_SAY", "|cffffffff|Hitem:9999::::|h[不明之物]|h|r")
Check("a link is named as this client names the thing, where the game knows it", Frames(), {
    "1 | [Bob] says:  | 收|cffffffff[Mageweave Cloth]|r 和 |cffffff00[[37] Stromgarde Badges]|r | ffffff | Bob",
    "1 | [Bob] says:  | |cffffffff[不明之物]|r | ffffff | Bob",
})

-- A link the game names a moment later: the line reads differently when the strip
-- repeats it, under the same number.
numbered, ns.db.repeatLines = true, 1
Chat("CHAT_MSG_SAY", "|cffffffff|Hitem:7777::::|h[符文布]|h|r")
itemNames[7777] = "Runecloth"
table.remove(timers, 1)()
Check("the line goes out as it is, and is repeated with the name once the game has it",
    { frames[1]:match("(%[[^%[]-%])|r"), frames[2]:match("(%[[^%[]-%])|r"), tostring(frames[1]:match("%d+$") == frames[2]:match("%d+$")) },
    { "[符文布]", "[Runecloth]", "true" })
numbered, ns.db.repeatLines, frames = false, 0, {}
Frames()

-- The strip's queue ----------------------------------------------------------------------

ns.SendToStrip("0\t\tfirst")
ns.SendToStrip("0\t\tsecond")
ChatFrame2.scripts.OnMouseWheel(nil, 1)
ChatFrame2.scripts.OnMouseWheel(nil, 1)
ChatFrame1.scripts.OnMouseWheel(nil, -1)
ns.SendToStrip("L\tolder", "L\t")
ns.SendToStrip("L\tnewer", "L\t")
Check("state jumps the queue and replaces older state of its kind", Frames(), {
    "0 |  | first",
    "L | newer\nS | General | -1\nS | Pick | 2\n0 |  | second",
})

-- Chat lines are numbered, and the last of them are shown again while nothing waits.
numbered, ns.db.repeatLines = true, 2
for _, text in ipairs({ "one", "two", "three" }) do Chat("CHAT_MSG_SAY", text) end
table.remove(timers, 1)()
local first = table.concat(frames, "\n")
local one, two, three = first:match("one.- | (%d+)\n.-two.- | (%d+)\n.-three.- | (%d+)$")
Check("chat lines end in numbers that start at the time of login and rise",
    { one:sub(1, 10), two - one, three - two }, { "1700000000", 1, 1 })
frames = {}
table.remove(timers, 1)()
table.remove(timers, 1)()
Check("the last lines are shown again, and again", { frames[1], frames[2] },
    { first:match("\n(.*)$"), first:match("\n(.*)$") })
ns.SendToStrip("0\t\tnew")
table.remove(timers, 1)()
Check("a new line goes first", frames[3], "0 |  | new")
numbered, ns.db.repeatLines, frames = false, 0, {}
Frames()

local symbols = encode(7, "hi")
Check("a frame fills the strip", #symbols, ns.STRIP.cols * ns.STRIP.rows)
Check("and starts with the sync blocks", table.concat({ symbols[1], symbols[2], symbols[3] }, ","), "51,15,60")

Check("the strip is drawn at half brightness, HDR or not", ns.StripBrightness(), 0.5)

-- Buttons and slash commands -------------------------------------------------------------

local slash = SlashCmdList.TRANSLATE
slash("")
slash("/g sure, I can heal")
slash("/w Ann on my way")
slash("help me please")
Check("replies and the button go to the companion", Frames(), {
    "C | reply",
    "R | help me please\nR | on my way\nR | sure, I can heal",
})
Check("and the chat box reopens on the target", opened, { "/g ", "/w Ann ", "" })

printed = {}
slash("/g")
slash("/w Ann")
Check("a target with nothing to say is refused", #printed, 2)
Check("nothing was sent for it", Frames(), {})

slash("/test 2")
Check("counted test lines go to the overlay, worded and colored like real chat", Frames(), {
    "1 | [Party] [|cffffffff奶媽小美|r]:  | 缺坦，來個戰士就開 (Translate test) | ffffff | 奶媽小美\n"
    .. "1 | [Guild] [|cff3fc7eb冰霜法爺|r]:  | 今晚團本幾點開？ (Translate test) | 40ff40 | 冰霜法爺" })
slash("/test 6")
local sixth = table.concat(Frames(), "\n"):match("[^\n]*$")
Check("a channel line names its channel and carries an item link", {
    tostring(sixth:find("[1. 綜合] [", 1, true) ~= nil), tostring(sixth:find("|cffa335ee[", 1, true) ~= nil), -- the link, as its name in epic purple
    tostring(sixth:match("| (%x+) | 德魯伊老王$")),
}, { "true", "true", "ffbfbf" })
Check("and are not said in chat", #said, 0)
slash("/TEST")
Check("the plain test line is said", said[1], "SAY 大家好，今天一起打副本嗎？ (Translate test)")
Frames()

local target = "小明"
function UnitName(unit) return unit == "target" and target or nil end
slash("/name")
slash("/name 黑熊 Lz")
target = nil
printed = {}
slash("/NAME")
Check("a name to explain: the target's, or one typed", Frames(), { "N |  |  | ffffff | 小明", "N |  |  | ffffff | 黑熊 Lz" })
Check("with neither, it asks whose", #printed, 1)

Check("SLASH aliases", SLASH_TRANSLATE1 .. " " .. SLASH_TRANSLATE3, "/tr /nms")
local numbers = {}
for _, n in ipairs({ 1, 9, 10, 11, 20, 21, 99 }) do numbers[#numbers + 1] = ns.ChineseNumber(n) end
Check("Chinese numbers", table.concat(numbers, " "), "一 九 十 十一 二十 二十一 九十九")

-- The glossary: notes after WoW terms, and a whole line's English for a line said often.
local function Notes(text) return (text:gsub("|Haddon:Translate:G:%d+|h|cff9d9d9d(%b())|r|h", "%1")) end
ns.GLOSSARY_LINES["等我一下"] = "Wait for me a moment"
Check("terms get their English after them", Notes(ns.Gloss("缺坦來個戰士", 5)),
    "缺坦" .. "(" .. ns.GLOSSARY_TERMS["缺坦"] .. ")" .. "來個戰士(" .. ns.GLOSSARY_TERMS["戰士"] .. ")")
Check("a line said often gets its whole English", Notes(ns.Gloss(" 等我一下 ", 5)), " 等我一下  (Wait for me a moment)")
Check("a Latin term only as a whole word", Notes(ns.Gloss("adds DD", 5)), "adds DD(" .. ns.GLOSSARY_TERMS["DD"] .. ")")
local link = "|cffa335ee|Hitem:19019::::|h[戰士之劍]|h|r"
Check("links and colors are left alone", Notes(ns.Gloss(link .. "缺坦", 5)), link .. "缺坦(" .. ns.GLOSSARY_TERMS["缺坦"] .. ")")
Check("the notes are taken out of a copied line", (ns.Gloss("缺坦", 5):gsub(" *|Haddon:Translate:%a:%d+|h.-|h", "")), "缺坦")

-- Choosing the copied chat window, as the companion's picker has the player paste.
local before = ns.db.sourceWindow
printed = {}
SlashCmdList.TRANSLATE("/window pick")
Check("/tr /window picks a window, whatever its case", ns.db.sourceWindow, "Pick")
Check("and the companion is told", ns.SourceLine(), "W\tPick")
SlashCmdList.TRANSLATE("/window nowhere")
Check("a window the game does not have changes nothing", ns.db.sourceWindow, "Pick")
SlashCmdList.TRANSLATE("/window all")
Check("all chat again", { ns.db.sourceWindow, ns.SourceLine() }, { "", "W\t" })
ns.db.sourceWindow = before

if failures > 0 then
    print(failures .. " failed")
    os.exit(1)
end
print("addon tests passed")
