local _, ns = ...

-- Tells the companion where the chat windows are, so its overlay can sit on
-- one of them and hide while that window's tab is not selected.
--
-- One strip line: "L", a tab, then one record per chat window. Records are
-- separated by byte 30 and their fields by byte 31:
--   name, shown (1 or 0), left, top, width, height, font size
-- Lengths are physical screen pixels and positions are measured from the
-- top-left corner of the game's picture, where the strip is drawn.

local FIELD, RECORD = "\31", "\30"
local CHECK_TIME = 0.25 -- seconds between looks at the chat windows
local RESEND_TIME = 5   -- resend unchanged, for a companion started later

local function Round(value, step)
    return math.floor(value / step + 0.5) * step
end

-- Pure Lua, so it can be tested outside WoW.
function ns.LayoutRecord(name, shown, left, top, width, height, fontSize)
    name = name:gsub("[%z\1-\31]", "")
    return table.concat({ name, shown and 1 or 0, left, top, width, height, fontSize }, FIELD)
end

local function Describe()
    local _, screenHeight = GetPhysicalScreenSize()
    local records = {}
    for i = 1, NUM_CHAT_WINDOWS or 10 do
        local frame = _G["ChatFrame" .. i]
        local name, fontSize = FCF_GetChatWindowInfo(i)
        if frame and type(name) == "string" and name ~= "" and (frame:IsShown() or frame.isDocked) then
            -- The background reaches a little past the text area on every side.
            local area = frame.Background or frame
            local left, top = area:GetLeft(), area:GetTop()
            if left and top then
                -- At effective scale 1 the screen is 768 units tall.
                local pixels = area:GetEffectiveScale() * screenHeight / 768
                records[#records + 1] = ns.LayoutRecord(
                    name, frame:IsVisible(),
                    Round(left * pixels, 1), Round(screenHeight - top * pixels, 1),
                    Round(area:GetWidth() * pixels, 1), Round(area:GetHeight() * pixels, 1),
                    Round((fontSize or 14) * frame:GetEffectiveScale() * screenHeight / 768, 0.1))
            end
        end
    end
    return "L\t" .. table.concat(records, RECORD)
end

-- The chat window whose lines are copied to the overlay, as a second line
-- after the layout: "W\t<name>", with no name for all chat. The companion
-- shows it beside its picker, which can only hand the player a /tr /window
-- command to paste.
function ns.SourceLine()
    local source = ns.db and ns.db.sourceWindow
    if type(source) ~= "string" then source = "" end
    return "W\t" .. source:gsub("[%z\1-\31]", "")
end

-- Mouse wheel steps over each chat window, counted since login: up adds,
-- down subtracts. The overlay cannot receive the wheel itself (clicks and
-- scrolling pass through it to the game), so with the overlay on a chat tab
-- the game window underneath gets the wheel and the count is sent as
-- "S\t<name>\t<count>" for the overlay to scroll by the difference. A count,
-- not single steps, so a newer one can replace one still waiting.
local wheel = {}

function ns.WheelMoved(index, delta)
    local name = FCF_GetChatWindowInfo(index)
    if type(name) ~= "string" or name == "" or type(delta) ~= "number" then return end
    name = name:gsub("[%z\1-\31]", "")
    wheel[index] = (wheel[index] or 0) + delta
    local key = "S\t" .. name .. "\t"
    ns.SendToStrip(key .. wheel[index], key)
end

local function WatchWheel()
    local hooked = 0
    for i = 1, NUM_CHAT_WINDOWS or 10 do
        local frame = _G["ChatFrame" .. i]
        if frame and frame.HookScript then
            frame:HookScript("OnMouseWheel", function(_, delta) ns.WheelMoved(i, delta) end)
            hooked = hooked + 1
        end
    end
    local on = not GetCVarBool or GetCVarBool("chatMouseScroll")
    ns.wheelSource = hooked .. " chat windows hooked" .. (on and "" or ", but mouse wheel scrolling is off in the game's chat options")
end

function ns.InitLayout()
    if not (C_Timer and C_Timer.NewTicker and FCF_GetChatWindowInfo) then
        ns.layoutSource = "none (no chat window API)"
        return
    end
    ns.layoutSource = "FCF_GetChatWindowInfo"
    local ok, problem = pcall(WatchWheel)
    if not ok then ns.wheelSource = "failed: " .. tostring(problem) end
    local last, sentAt = nil, 0
    local ticker
    ticker = C_Timer.NewTicker(CHECK_TIME, function()
        local ok, text = pcall(function() return Describe() .. "\n" .. ns.SourceLine() end)
        if not ok then
            ns.layoutSource = "failed: " .. tostring(text)
            ns.Print("Could not read the chat windows, so the overlay cannot follow them: " .. tostring(text))
            ticker:Cancel()
            return
        end
        local now = GetTime()
        if text ~= last or now - sentAt >= RESEND_TIME then
            last, sentAt = text, now
            ns.SendToStrip(text, "L\t")
        end
    end)
end
