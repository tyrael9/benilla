local addonName = "MinimapMover"

-- Account-wide SavedVariables: all characters share one minimap position.
MinimapMoverDB = MinimapMoverDB or {}
MinimapMoverDB.unlocked = MinimapMoverDB.unlocked or false

-- Move the entire cluster, but receive drag input from the minimap itself.
local mover = MinimapCluster or Minimap

local function Print(msg)
    DEFAULT_CHAT_FRAME:AddMessage("|cff66ccff[MinimapMover]|r " .. msg)
end

local function SavePosition()
    local point, relativeTo, relativePoint, x, y = mover:GetPoint(1)

    MinimapMoverDB.point = point
    MinimapMoverDB.relativePoint = relativePoint
    MinimapMoverDB.x = x
    MinimapMoverDB.y = y
end

local function RestorePosition()
    if not MinimapMoverDB.point then
        return
    end

    mover:ClearAllPoints()
    mover:SetPoint(
        MinimapMoverDB.point,
        UIParent,
        MinimapMoverDB.relativePoint or MinimapMoverDB.point,
        MinimapMoverDB.x or 0,
        MinimapMoverDB.y or 0
    )
end

local function SetUnlocked(unlocked, quiet)
    MinimapMoverDB.unlocked = unlocked and true or false

    mover:SetMovable(MinimapMoverDB.unlocked)

    -- Keep drag input on Minimap so clicks on the map initiate dragging.
    Minimap:RegisterForDrag("LeftButton")

    if MinimapMoverDB.unlocked then
        if not quiet then
            Print("unlocked — drag the minimap with the left mouse button")
        end
    else
        mover:StopMovingOrSizing()

        if not quiet then
            Print("locked")
        end
    end
end

-- Vanilla 1.12 compatibility: HookScript may be unavailable.
local function HookScriptCompat(frame, scriptName, handler)
    if not frame or not frame.GetScript or not frame.SetScript then
        return
    end

    local oldHandler = frame:GetScript(scriptName)

    frame:SetScript(scriptName, function()
        if oldHandler then
            oldHandler()
        end

        handler()
    end)
end

-- Preserve existing minimap handlers while moving the entire cluster.
HookScriptCompat(Minimap, "OnDragStart", function()
    if MinimapMoverDB.unlocked then
        mover:StartMoving()
    end
end)

HookScriptCompat(Minimap, "OnDragStop", function()
    if MinimapMoverDB.unlocked then
        mover:StopMovingOrSizing()
        SavePosition()
    end
end)

SLASH_MINIMAPMOVER1 = "/mm"
SLASH_MINIMAPMOVER2 = "/minimap"

SlashCmdList["MINIMAPMOVER"] = function(msg)
    msg = string.lower(msg or "")

    if msg == "unlock" or msg == "on" then
        SetUnlocked(true, false)

    elseif msg == "lock" or msg == "off" then
        SetUnlocked(false, false)

    elseif msg == "reset" then
        MinimapMoverDB.point = nil
        MinimapMoverDB.relativePoint = nil
        MinimapMoverDB.x = nil
        MinimapMoverDB.y = nil

        mover:ClearAllPoints()
        mover:SetPoint("TOPRIGHT", UIParent, "TOPRIGHT", -25, -25)

        SetUnlocked(MinimapMoverDB.unlocked, true)
        Print("position reset")

    else
        Print("/mm unlock  - make minimap draggable")
        Print("/mm lock    - lock minimap")
        Print("/mm reset   - restore default position")
    end
end

local frame = CreateFrame("Frame")
frame:RegisterEvent("PLAYER_LOGIN")

frame:SetScript("OnEvent", function()
    -- Restore after Blizzard finishes anchoring the minimap cluster.
    if MinimapMoverDB.point then
        RestorePosition()
    end

    SetUnlocked(MinimapMoverDB.unlocked, true)
end)
