-- ============================================================
-- Benilla Controller
-- Vanilla 1.12 ActionButton based controller bar
--
-- Physical layout:
--
--                 Y
--             X       B
--                 A
--
--                 UP
--             LEFT    RIGHT
--                DOWN
--
-- Base : Action  1 -  8
-- L    : Action  9 - 16
-- R    : Action 17 - 24
--
-- L/R are HOLD modifiers.
-- They change the 8 displayed buttons without changing
-- the actual WoW action bar page.
-- ============================================================


-- ============================================================
-- Configuration
-- ============================================================

local SLOT_SIZE = 36
local SLOT_SPACING = 6

local BAR_X = 20
local BAR_Y = 100

local UPDATE_INTERVAL = 0.10

local SHOW_HOTKEYS = true
local SHOW_COUNTS = true
local SHOW_EMPTY_SLOTS = true

local RANGE_COLOR_R = 1.0
local RANGE_COLOR_G = 0.10
local RANGE_COLOR_B = 0.10


-- ============================================================
-- Current modifier
--
-- 0 = base   (1-8)
-- 1 = L      (9-16)
-- 2 = R      (17-24)
-- ============================================================

local currentModifier = 0


-- ============================================================
-- Controller labels
-- ============================================================

local controllerLabels = {
    [1] = "A",
    [2] = "B",
    [3] = "X",
    [4] = "Y",

    [5] = "UP",
    [6] = "DOWN",
    [7] = "LEFT",
    [8] = "RIGHT"
}


local function GetControllerLabel(slot)

    local label = controllerLabels[slot]

    if not label then
        return ""
    end

    if currentModifier == 1 then
        return "L+" .. label
    end

    if currentModifier == 2 then
        return "R+" .. label
    end

    return label
end


-- ============================================================
-- Convert physical controller slot to actual Action slot
-- ============================================================

local function GetDisplayActionSlot(controllerSlot)

    if currentModifier == 1 then
        return controllerSlot + 8
    end

    if currentModifier == 2 then
        return controllerSlot + 16
    end

    return controllerSlot
end


-- ============================================================
-- Layout
-- ============================================================

local LAYOUT_WIDTH =
    SLOT_SIZE * 8
    + SLOT_SPACING * 7

local LAYOUT_HEIGHT =
    SLOT_SIZE * 3
    + SLOT_SPACING * 2


local controllerPositions = {
    [1] = { x = SLOT_SIZE * 5 + SLOT_SPACING * 5, y = 0 },
    [2] = { x = SLOT_SIZE * 6 + SLOT_SPACING * 6, y = SLOT_SIZE + SLOT_SPACING },
    [3] = { x = SLOT_SIZE * 4 + SLOT_SPACING * 4, y = SLOT_SIZE + SLOT_SPACING },
    [4] = { x = SLOT_SIZE * 5 + SLOT_SPACING * 5, y = SLOT_SIZE * 2 + SLOT_SPACING * 2 },
    [5] = { x = SLOT_SIZE * 1 + SLOT_SPACING, y = SLOT_SIZE * 2 + SLOT_SPACING * 2 },
    [6] = { x = SLOT_SIZE * 1 + SLOT_SPACING, y = 0 },
    [7] = { x = 0, y = SLOT_SIZE + SLOT_SPACING },
    [8] = { x = SLOT_SIZE * 2 + SLOT_SPACING * 2, y = SLOT_SIZE + SLOT_SPACING }
}


-- ============================================================
-- Main bar
-- ============================================================

local BenillaControllerBar =
    CreateFrame(
        "Frame",
        "BenillaControllerBar",
        UIParent
    )

BenillaControllerBar:SetWidth(
    LAYOUT_WIDTH
)

BenillaControllerBar:SetHeight(
    LAYOUT_HEIGHT
)

BenillaControllerBar:SetPoint(
    "BOTTOM",
    UIParent,
    "BOTTOM",
    BAR_X,
    BAR_Y
)

BenillaControllerBar:SetFrameStrata("LOW")
BenillaControllerBar:Hide()


-- ============================================================
-- Cooldown Model
-- ============================================================

function BenillaController_CooldownFrame_SetTimer(
    this,
    start,
    duration,
    enable
)

    if start > 0
        and duration > 0
        and enable > 0 then

        this.cooldown_start = start
        this.cooldown_duration = duration
        this.cooldown_stopping = 0

        this:SetSequence(0)
        this:Show()

    else

        this.cooldown_start = 0
        this.cooldown_duration = 0
        this.cooldown_stopping = 0

        this:Hide()
    end
end


function BenillaController_CooldownFrame_OnUpdateModel()

    if this.cooldown_stopping == 0 then

        if not this.cooldown_start
            or not this.cooldown_duration
            or this.cooldown_duration <= 0 then

            this:Hide()
            return
        end

        local finished =
            (GetTime() - this.cooldown_start)
            / this.cooldown_duration

        if finished < 1.0 then

            local time =
                finished * 1000

            this:SetSequenceTime(
                0,
                time
            )

            return
        end

        this.cooldown_stopping = 1

        this:SetSequence(1)
        this:SetSequenceTime(1, 0)

    else

        this:AdvanceTime()
    end
end


function BenillaController_CooldownFrame_OnAnimFinished()

    if this.cooldown_stopping == 1 then
        this:Hide()
    end
end


-- ============================================================
-- Get standard ActionButton child
-- ============================================================

local function GetButtonPart(
    button,
    suffix
)

    local name =
        button:GetName()

    if not name then
        return nil
    end

    return getglobal(
        name .. suffix
    )
end


-- ============================================================
-- Create one physical controller button
-- ============================================================

local function CreateControllerButton(
    controllerSlot
)

    local button =
        CreateFrame(
            "CheckButton",
            "BenillaControllerButton" .. controllerSlot,
            BenillaControllerBar,
            "ActionButtonTemplate"
        )

    button.controllerSlot =
        controllerSlot

    button.actionSlot =
        controllerSlot


    -- --------------------------------------------------------
    -- Position
    -- --------------------------------------------------------

    local position =
        controllerPositions[
            controllerSlot
        ]

    button:SetWidth(
        SLOT_SIZE
    )

    button:SetHeight(
        SLOT_SIZE
    )

    button:SetPoint(
        "BOTTOMLEFT",
        BenillaControllerBar,
        "BOTTOMLEFT",
        position.x,
        position.y
    )


    -- --------------------------------------------------------
    -- Move the standard HotKey to the top center.
    -- --------------------------------------------------------

    local hotkey =
        GetButtonPart(
            button,
            "HotKey"
        )

    if hotkey then

        hotkey:ClearAllPoints()

        hotkey:SetPoint(
            "BOTTOM",
            button,
            "TOP",
            0,
            1
        )

        hotkey:SetFont(
            "Fonts\\FRIZQT__.TTF",
            9,
            "OUTLINE"
        )
    end


    -- --------------------------------------------------------
    -- Drag
    -- --------------------------------------------------------

    button:RegisterForDrag(
        "LeftButton",
        "RightButton"
    )


    -- --------------------------------------------------------
    -- Click
    -- --------------------------------------------------------

    button:RegisterForClicks(
        "LeftButtonUp",
        "RightButtonUp"
    )


    button:SetScript(
        "OnClick",
        function()

            local action =
                button.actionSlot

            if not action then
                return
            end

            -- A payload already exists on the WoW cursor.
            -- Treat this click as a native action-bar drop.
            if CursorHasSpell()
                or CursorHasItem()
                or CursorHasAction() then

                UseAction(action, 1, 0)

                return
            end


            -- ----------------------------------------------------
            -- Normal action-button behavior
            -- ----------------------------------------------------

            if arg1 == "RightButton" then

                UseAction(
                    action,
                    0,
                    1
                )

            else

                UseAction(
                    action,
                    0,
                    0
                )
            end
        end
    )


    -- --------------------------------------------------------
    -- Drag start
    -- --------------------------------------------------------

    button:SetScript(
        "OnDragStart",
        function()

            local action =
                button.actionSlot

            if not action then
                return
            end

            PickupAction(action)
        end
    )


    -- --------------------------------------------------------
    -- Receive drag
    -- --------------------------------------------------------

    button:SetScript(
        "OnReceiveDrag",
        function()

            local action =
                button.actionSlot

            if not action then
                return
            end

            PlaceAction(action)
        end
    )


    -- --------------------------------------------------------
    -- Tooltip
    -- --------------------------------------------------------

    button:SetScript(
        "OnEnter",
        function()

            local action =
                button.actionSlot

            if not action then
                return
            end

            GameTooltip:SetOwner(
                this,
                "ANCHOR_RIGHT"
            )

            GameTooltip:SetAction(
                action
            )

            GameTooltip:Show()
        end
    )


    button:SetScript(
        "OnLeave",
        function()

            GameTooltip:Hide()
        end
    )


    -- --------------------------------------------------------
    -- Cooldown Model
    -- --------------------------------------------------------

    local cooldown =
        CreateFrame(
            "Model",
            nil,
            button
        )

    cooldown:SetWidth(
        SLOT_SIZE
    )

    cooldown:SetHeight(
        SLOT_SIZE
    )

    cooldown:SetPoint(
        "CENTER",
        button,
        "CENTER",
        0,
        0
    )

    cooldown:SetModel(
        "Interface\\Cooldown\\UI-Cooldown-Indicator.mdx"
    )

    cooldown:SetFrameStrata(
        "HIGH"
    )

    cooldown:SetFrameLevel(
        button:GetFrameLevel() + 10
    )

    cooldown:SetScript(
        "OnUpdateModel",
        BenillaController_CooldownFrame_OnUpdateModel
    )

    cooldown:SetScript(
        "OnAnimFinished",
        BenillaController_CooldownFrame_OnAnimFinished
    )

    cooldown:Hide()

    button.controllerCooldown =
        cooldown


    -- --------------------------------------------------------
    -- Cooldown number
    -- --------------------------------------------------------

    local cooldownText =
        button:CreateFontString(
            nil,
            "OVERLAY"
        )

    cooldownText:SetFont(
        "Fonts\\FRIZQT__.TTF",
        14,
        "OUTLINE"
    )

    cooldownText:SetPoint(
        "CENTER",
        button,
        "CENTER",
        0,
        0
    )

    cooldownText:SetText("")
    cooldownText:Hide()

    button.controllerCooldownText =
        cooldownText


    return button
end


-- ============================================================
-- Create exactly 8 physical buttons
-- ============================================================

local controllerButtons = {}

local slot = 1

while slot <= 8 do

    controllerButtons[slot] =
        CreateControllerButton(slot)

    slot = slot + 1
end


-- ============================================================
-- Update one physical button
-- ============================================================

local function UpdateControllerButton(
    button
)

    local controllerSlot =
        button.controllerSlot

    local action =
        GetDisplayActionSlot(
            controllerSlot
        )

    button.actionSlot =
        action


    -- --------------------------------------------------------
    -- HotKey
    -- --------------------------------------------------------

    local hotkey =
        GetButtonPart(
            button,
            "HotKey"
        )

    if hotkey then

        if SHOW_HOTKEYS then

            hotkey:SetText(
                GetControllerLabel(
                    controllerSlot
                )
            )

            hotkey:Show()

        else

            hotkey:SetText("")
            hotkey:Hide()
        end
    end


    -- --------------------------------------------------------
    -- Icon
    -- --------------------------------------------------------

    local icon =
        GetButtonPart(
            button,
            "Icon"
        )

    if icon then

        local texture =
            GetActionTexture(action)

        if texture then

            icon:SetTexture(
                texture
            )

            icon:Show()

        else

            icon:SetTexture(nil)

            if SHOW_EMPTY_SLOTS then
                icon:Show()
            else
                icon:Hide()
            end
        end
    end


    -- --------------------------------------------------------
    -- Count
    -- --------------------------------------------------------

    local count =
        GetButtonPart(
            button,
            "Count"
        )

    if count then

        if SHOW_COUNTS
            and IsConsumableAction(action) then

            count:SetText(
                tostring(
                    GetActionCount(action)
                )
            )

            count:Show()

        else

            count:SetText("")
            count:Hide()
        end
    end


    -- --------------------------------------------------------
    -- Empty action
    -- --------------------------------------------------------

    if not HasAction(action) then

        if not SHOW_EMPTY_SLOTS then

            if icon then
                icon:Hide()
            end
        end

        button.controllerCooldown:Hide()

        button.controllerCooldownText:SetText("")
        button.controllerCooldownText:Hide()

        return
    end


    -- --------------------------------------------------------
    -- ActionButton state
    --
    -- Vanilla 1.12:
    --
    -- IsCurrentAction()
    -- IsAutoRepeatAction()
    --
    -- drives the checked/pressed state.
    --
    -- This is what makes queued abilities such as
    -- Warrior Heroic Strike highlight correctly.
    -- --------------------------------------------------------

    local isCurrent =
        IsCurrentAction(action)

    local isAutoRepeat =
        IsAutoRepeatAction(action)

    if isCurrent or isAutoRepeat then

        button:SetChecked(1)

    else

        button:SetChecked(0)
    end


    -- --------------------------------------------------------
    -- Usable / range
    --
    -- Copy the Vanilla 1.12 ActionButton semantics:
    --
    -- usable          -> white
    -- not enough rage -> blue
    -- out of range    -> red
    -- otherwise       -> gray
    --
    -- We intentionally keep our existing red range color.
    -- --------------------------------------------------------

    if icon then

        local usable, noMana =
            IsUsableAction(action)

        local consumable =
            IsConsumableAction(action)

        local actionCount =
            GetActionCount(action)

        if consumable
            and actionCount == 0 then

            icon:SetVertexColor(
                0.45,
                0.45,
                0.45
            )

        elseif usable then

            local inRange =
                IsActionInRange(action)

            if inRange == 0 then

                icon:SetVertexColor(
                    RANGE_COLOR_R,
                    RANGE_COLOR_G,
                    RANGE_COLOR_B
                )

            else

                icon:SetVertexColor(
                    1.0,
                    1.0,
                    1.0
                )
            end

        elseif noMana then

            -- Vanilla ActionButton's "not enough
            -- mana/rage/energy" state.
            icon:SetVertexColor(
                0.5,
                0.5,
                1.0
            )

        else

            icon:SetVertexColor(
                0.4,
                0.4,
                0.4
            )
        end
    end


    -- --------------------------------------------------------
    -- Cooldown
    -- --------------------------------------------------------

    local start, duration, enable =
        GetActionCooldown(action)

    if start
        and duration
        and enable
        and start > 0
        and duration > 0
        and enable > 0 then

        BenillaController_CooldownFrame_SetTimer(
            button.controllerCooldown,
            start,
            duration,
            enable
        )


        local remaining =
            duration
            - (GetTime() - start)

        if remaining < 0 then
            remaining = 0
        end


        if remaining >= 10 then

            local whole =
                math.floor(
                    remaining
                )

            button.controllerCooldownText:SetText(
                tostring(whole)
            )

        elseif remaining > 0 then

            local tenths =
                math.floor(
                    remaining * 10
                )

            local whole =
                math.floor(
                    tenths / 10
                )

            local decimal =
                tenths
                - whole * 10

            button.controllerCooldownText:SetText(
                tostring(whole)
                .. "."
                .. tostring(decimal)
            )

        else

            button.controllerCooldownText:SetText("")
        end


        button.controllerCooldownText:Show()

    else

        button.controllerCooldown:Hide()

        button.controllerCooldownText:SetText("")
        button.controllerCooldownText:Hide()
    end
end


-- ============================================================
-- Update all 8 physical buttons
-- ============================================================

function UpdateAllSlots()

    local slot = 1

    while slot <= 8 do

        local button =
            controllerButtons[slot]

        if button then

            UpdateControllerButton(
                button
            )
        end

        slot = slot + 1
    end
end


-- ============================================================
-- Modifier interface
--
-- Called by native controller input:
--
-- 0 = base
-- 1 = L
-- 2 = R
-- ============================================================

function BenillaController_SetModifier(
    modifier
)

    local newModifier = 0

    if modifier == 1 then

        newModifier = 1

    elseif modifier == 2 then

        newModifier = 2
    end


    if currentModifier ==
        newModifier then

        return
    end


    currentModifier =
        newModifier

    UpdateAllSlots()
end


-- ============================================================
-- Controller slot command
-- ============================================================

function BenillaController_Slot(
    action
)

    if not action then
        return
    end

    UseAction(
        action,
        0,
        0
    )
end


-- ============================================================
-- Update timer
--
-- Benilla passes elapsed through global arg1.
-- ============================================================

local controllerVisible = false

local function UpdateControllerVisibility()

    local connected = IsGamepadConnected()

    if connected and not controllerVisible then

        controllerVisible = true
        BenillaControllerBar:Show()

        UpdateAllSlots()

    elseif not connected and controllerVisible then

        controllerVisible = false
        BenillaControllerBar:Hide()
    end

    return connected
end

local updateElapsed = 0

local eventFrame =
    CreateFrame(
        "Frame",
        "BenillaControllerUpdateFrame",
        UIParent
    )


eventFrame:SetScript(
    "OnUpdate",
    function()

        if not UpdateControllerVisibility() then
            return
        end

        local elapsed = arg1

        if not elapsed then
            return
        end

        updateElapsed =
            updateElapsed + elapsed

        if updateElapsed <
            UPDATE_INTERVAL then

            return
        end

        updateElapsed = 0

        UpdateAllSlots()
    end
)


-- ============================================================
-- Slash commands
-- ============================================================

SLASH_BENILLACONTROLLER1 =
    "/bc"


SlashCmdList["BENILLACONTROLLER"] =
    function(message)

        if message == "up" then

            BAR_Y =
                BAR_Y + 10

            BenillaControllerBar:ClearAllPoints()

            BenillaControllerBar:SetPoint(
                "BOTTOM",
                UIParent,
                "BOTTOM",
                BAR_X,
                BAR_Y
            )

        elseif message == "down" then

            BAR_Y =
                BAR_Y - 10

            BenillaControllerBar:ClearAllPoints()

            BenillaControllerBar:SetPoint(
                "BOTTOM",
                UIParent,
                "BOTTOM",
                BAR_X,
                BAR_Y
            )

        elseif message == "reload" then

            ReloadUI()
        end
    end


-- ============================================================
-- Initial state
-- ============================================================

currentModifier = 0

UpdateControllerVisibility()