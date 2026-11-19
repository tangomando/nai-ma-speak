local _, ns = ...

-- The pixel strip: the only live way out of the game. New Chinese lines are
-- drawn as a small grid of colored blocks that the companion app reads off
-- the screen. Each block carries 6 bits: 4 brightness levels per channel,
-- 64 colors in all.
--
-- Frame layout, row by row from the top-left block:
--   3 sync blocks (magenta, cyan, yellow), then the payload bytes
--   [seq] [length hi] [length lo] [text...] [fletcher16 hi] [fletcher16 lo]
--   packed 3 bytes -> 4 blocks, padded with black.
-- The text holds one or more chat lines, separated by newlines.
--
-- Under the frame sits the palette: one more row with the 64 colors in order.
-- A display's color profile, the game's gamma setting and the like change
-- every color on the way to the companion; it reads a block by finding the
-- palette color it looks like, so it need not know what was changed.
local COLS, ROWS = 64, 8 -- of the frame; the palette row comes after
local BLOCK = 4 -- physical screen pixels per block side
local SYNC = { 51, 15, 60 } -- (3,0,3) magenta, (0,3,3) cyan, (3,3,0) yellow
local CAPACITY = math.floor((COLS * ROWS - #SYNC) / 4) * 3
local MAX_TEXT = CAPACITY - 5
local SHOW_TIME = 0.25 -- seconds each frame stays up; the app polls faster
local MAX_QUEUE = 200

ns.STRIP = { cols = COLS, rows = ROWS, block = BLOCK, maxText = MAX_TEXT }

local function Fletcher16(bytes)
    local a, b = 0, 0
    for i = 1, #bytes do
        a = (a + bytes[i]) % 255
        b = (b + a) % 255
    end
    return b * 256 + a
end

-- Cuts at a UTF-8 character boundary so a long line stays valid text.
local function Truncate(text, max)
    if #text <= max then return text end
    local cut = max
    while cut > 0 do
        local c = text:byte(cut + 1)
        if c < 128 or c >= 192 then break end
        cut = cut - 1
    end
    return text:sub(1, cut)
end

ns.Truncate = Truncate

-- Returns the frame's COLS * ROWS symbols (0..63). Pure Lua, so it can be tested outside WoW.
function ns.EncodeFrame(seq, text)
    text = Truncate(text, MAX_TEXT)
    local bytes = { seq % 256, math.floor(#text / 256), #text % 256 }
    for i = 1, #text do bytes[#bytes + 1] = text:byte(i) end
    local sum = Fletcher16(bytes)
    bytes[#bytes + 1] = math.floor(sum / 256)
    bytes[#bytes + 1] = sum % 256

    local symbols = {}
    for i = 1, #SYNC do symbols[i] = SYNC[i] end
    for i = 1, #bytes, 3 do
        local v = (bytes[i] or 0) * 65536 + (bytes[i + 1] or 0) * 256 + (bytes[i + 2] or 0)
        symbols[#symbols + 1] = math.floor(v / 262144) % 64
        symbols[#symbols + 1] = math.floor(v / 4096) % 64
        symbols[#symbols + 1] = math.floor(v / 64) % 64
        symbols[#symbols + 1] = v % 64
    end
    for i = #symbols + 1, COLS * ROWS do symbols[i] = 0 end
    return symbols
end

local frame, textures
local queue = {}
-- Lines shown again, in turn, while the queue is empty: the last
-- `repeatLines` given to RepeatOnStrip, each a function that returns the
-- line's text. `turn` is the one shown last.
local again, turn = {}, 0
local seq = 0
local ticking = false

-- One block = one physical pixel grid cell, whatever the UI scale.
function ns.PlaceStrip()
    if not frame then return end
    local _, height = GetPhysicalScreenSize()
    frame:SetScale(768 / height)
    frame:ClearAllPoints()
    frame:SetPoint("TOPLEFT", UIParent, "TOPLEFT", 0, 0)
end

local function Build()
    frame = CreateFrame("Frame", "TranslateStrip", UIParent)
    frame:SetIgnoreParentScale(true)
    frame:SetFrameStrata("TOOLTIP")
    frame:SetSize(COLS * BLOCK, (ROWS + 1) * BLOCK)
    textures = {}
    for i = 0, COLS * (ROWS + 1) - 1 do
        local t = frame:CreateTexture(nil, "OVERLAY")
        t:SetSize(BLOCK, BLOCK)
        t:SetPoint("TOPLEFT", (i % COLS) * BLOCK, -math.floor(i / COLS) * BLOCK)
        textures[i + 1] = t
    end
    for color = 0, COLS - 1 do ns.PaintBlock(COLS * ROWS + color + 1, color) end
    ns.PlaceStrip()
end

-- The strip is drawn at half brightness, always. With the game's HDR output on, a
-- screen capture sees the picture brightened: at full brightness the top two of the
-- four levels came back both clipped to white (0, 85, 170, 255 read as 0, 149, 255,
-- 255). At half they stay apart whether or not the capture brightens them, so nothing
-- depends on noticing the HDR setting, which can be switched from the console at any
-- time. A dimmer strip is also less glaring in the corner of the screen. The companion
-- reads blocks by the palette row, dimmed the same way, so it needs to know nothing of
-- this.
local BRIGHTNESS = 0.5

function ns.StripBrightness()
    return BRIGHTNESS
end

-- Colors the block at `index` (from 1) with one of the 64 colors.
function ns.PaintBlock(index, color)
    local r, g, b = math.floor(color / 16) % 4, math.floor(color / 4) % 4, color % 4
    local top = 3 / BRIGHTNESS
    textures[index]:SetColorTexture(r / top, g / top, b / top, 1)
end

local function Paint(symbols)
    for i, s in ipairs(symbols) do ns.PaintBlock(i, s) end
end

local function ForgetOld()
    while #again > ns.db.repeatLines do
        table.remove(again, 1)
        turn = math.max(turn - 1, 0)
    end
end

-- The text of the next frame, or nil with nothing to show. Short lines share
-- a frame, so a busy channel does not fall behind.
local function NextText()
    local text = table.remove(queue, 1)
    if text then
        while queue[1] and #text + 1 + #queue[1] <= MAX_TEXT do
            text = text .. "\n" .. table.remove(queue, 1)
        end
        return text
    end
    ForgetOld()
    for _ = 1, #again do
        local line = again[turn % #again + 1]()
        if text and #text + 1 + #line > MAX_TEXT then break end
        text = text and (text .. "\n" .. line) or line
        turn = turn % #again + 1
    end
    return text
end

local function Advance()
    local text = NextText()
    if not text then
        ticking = false
        return
    end
    seq = (seq + 1) % 256
    Paint(ns.EncodeFrame(seq, text))
    ticking = true
    C_Timer.After(SHOW_TIME, Advance)
end

-- Text sent with a key is state, not a chat line: the chat window layout or a
-- window's scroll count. It jumps the queue, so the overlay follows the game
-- even with many lines waiting, and it replaces waiting text that starts
-- with the same key, since only the newest state matters.
function ns.SendToStrip(text, key)
    if not frame then Build() end
    if key then
        for i = #queue, 1, -1 do
            if queue[i]:sub(1, #key) == key then table.remove(queue, i) end
        end
        table.insert(queue, 1, text)
    else
        if #queue >= MAX_QUEUE then table.remove(queue, 1) end
        queue[#queue + 1] = text
    end
    if not ticking then Advance() end
end

-- Has the strip show a line again from time to time, once it has been sent.
-- `text` is called each time for the line as it then reads.
function ns.RepeatOnStrip(text)
    again[#again + 1] = text
    ForgetOld()
end

ns.On("DISPLAY_SIZE_CHANGED", function() ns.PlaceStrip() end)
ns.On("UI_SCALE_CHANGED", function() ns.PlaceStrip() end)
