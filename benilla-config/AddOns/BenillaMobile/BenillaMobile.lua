-- BenillaMobile
-- A touch-friendly action bar. It reads and activates existing WoW 1.12 action slots;
-- paging is local to this addon and never changes the stock action-bar page.

local BUTTON_COUNT = 12
local GRID_COLUMNS = 4
local GRID_ROWS = 3
local MAX_ACTION_SLOTS = 120
local UPDATE_INTERVAL = 0.12
-- The stock ActionButtonTemplate and its cooldown model are authored at 36 UI units.
-- Scale the whole control from this base so its fixed-size border textures and model mask
-- grow with the icon, instead of only enlarging the parent frame.
local BASE_BUTTON_SIZE = 36
local DEFAULT_BUTTON_SIZE = 96
local PREVIOUS_DEFAULT_BUTTON_SIZE_1 = 52
local PREVIOUS_DEFAULT_BUTTON_SIZE_2 = 68
local MAX_BUTTON_SIZE = 96
local BUTTON_SPACING = 6
local HEADER_HEIGHT = 26
local DEFAULT_X = 0
local DEFAULT_Y = 18

local RANGE_COLOR_R = 1.0
local RANGE_COLOR_G = 0.10
local RANGE_COLOR_B = 0.10

BenillaMobileDB = BenillaMobileDB or {}

local function InitializeSettings()
    if type(BenillaMobileDB) ~= "table" then
        BenillaMobileDB = {}
    end

    if type(BenillaMobileDB.page) ~= "number" then
        BenillaMobileDB.page = 1
    end
    if type(BenillaMobileDB.x) ~= "number" then
        BenillaMobileDB.x = DEFAULT_X
    end
    if type(BenillaMobileDB.y) ~= "number" then
        BenillaMobileDB.y = DEFAULT_Y
    end
    if type(BenillaMobileDB.buttonSize) ~= "number" then
        BenillaMobileDB.buttonSize = DEFAULT_BUTTON_SIZE
    end

    -- Move existing installs from the original 52-point default to the larger mobile default,
    -- but preserve a size the player already chose explicitly.
    if BenillaMobileDB.version ~= 3 then
        -- Upgrade installations still using either shipped default. Preserve other explicit sizes.
        if BenillaMobileDB.buttonSize == PREVIOUS_DEFAULT_BUTTON_SIZE_1
            or BenillaMobileDB.buttonSize == PREVIOUS_DEFAULT_BUTTON_SIZE_2 then
            BenillaMobileDB.buttonSize = DEFAULT_BUTTON_SIZE
        end
        BenillaMobileDB.version = 3
    end
    if BenillaMobileDB.visibility ~= "show"
        and BenillaMobileDB.visibility ~= "hide"
        and BenillaMobileDB.visibility ~= "auto" then
        BenillaMobileDB.visibility = "auto"
    end
    if BenillaMobileDB.autoHideWithGamepad == nil then
        BenillaMobileDB.autoHideWithGamepad = true
    end

    BenillaMobileDB.buttonSize =
        math.max(40, math.min(MAX_BUTTON_SIZE, BenillaMobileDB.buttonSize))
end

InitializeSettings()

local ACTIONS_PER_PAGE = BUTTON_COUNT
local PAGE_COUNT = math.ceil(MAX_ACTION_SLOTS / ACTIONS_PER_PAGE)

local function ClampPage(page)
    page = math.floor(tonumber(page) or 1)
    return math.max(1, math.min(PAGE_COUNT, page))
end

local function ActionSlotForButton(buttonIndex)
    return (BenillaMobileDB.page - 1) * ACTIONS_PER_PAGE + buttonIndex
end

local function ButtonSize()
    return BenillaMobileDB.buttonSize
end

local function GetButtonPart(button, suffix)
    local name = button:GetName()
    if not name then
        return nil
    end
    return getglobal(name .. suffix)
end

-- The reference cooldown indicator is a model overlay in this client.
function BenillaMobile_CooldownFrame_SetTimer(this, start, duration, enable)
    if start and duration and enable
        and start > 0 and duration > 0 and enable > 0 then
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

function BenillaMobile_CooldownFrame_OnUpdateModel()
    if this.cooldown_stopping == 0 then
        if not this.cooldown_start
            or not this.cooldown_duration
            or this.cooldown_duration <= 0 then
            this:Hide()
            return
        end

        local finished =
            (GetTime() - this.cooldown_start) / this.cooldown_duration

        if finished < 1.0 then
            this:SetSequenceTime(0, finished * 1000)
            return
        end

        this.cooldown_stopping = 1
        this:SetSequence(1)
        this:SetSequenceTime(1, 0)
    else
        this:AdvanceTime()
    end
end

function BenillaMobile_CooldownFrame_OnAnimFinished()
    if this.cooldown_stopping == 1 then
        this:Hide()
    end
end

local BAR_WIDTH =
    GRID_COLUMNS * DEFAULT_BUTTON_SIZE
    + (GRID_COLUMNS - 1) * BUTTON_SPACING

local BAR_HEIGHT =
    GRID_ROWS * DEFAULT_BUTTON_SIZE
    + (GRID_ROWS - 1) * BUTTON_SPACING
    + HEADER_HEIGHT

local bar = CreateFrame("Frame", "BenillaMobileBar", UIParent)
bar:SetWidth(BAR_WIDTH)
bar:SetHeight(BAR_HEIGHT)
bar:SetPoint(
    "BOTTOM",
    UIParent,
    "BOTTOM",
    BenillaMobileDB.x,
    BenillaMobileDB.y
)
bar:SetFrameStrata("MEDIUM")
bar:EnableMouse(false)

local header = CreateFrame("Frame", "BenillaMobileHeader", bar)
header:SetWidth(BAR_WIDTH)
header:SetHeight(HEADER_HEIGHT)
header:SetPoint("BOTTOMLEFT", bar, "TOPLEFT", 0, -HEADER_HEIGHT)

local function CreateHeaderButton(name, text, point, relativePoint, x)
    local button = CreateFrame("Button", name, header, "UIPanelButtonTemplate")
    button:SetWidth(58)
    button:SetHeight(22)
    button:SetPoint(point, header, relativePoint, x, 0)
    button:SetText(text)
    return button
end

local previousPageButton = CreateHeaderButton(
    "BenillaMobilePreviousPage", "<<", "LEFT", "LEFT", 0
)
local nextPageButton = CreateHeaderButton(
    "BenillaMobileNextPage", ">>", "RIGHT", "RIGHT", 0
)

local pageLabel = header:CreateFontString(
    "BenillaMobilePageLabel", "OVERLAY"
)
pageLabel:SetFont("Fonts\\FRIZQT__.TTF", 12, "OUTLINE")
pageLabel:SetPoint("CENTER", header, "CENTER", 0, 0)
pageLabel:SetText("1 / 10")

local mobileButtons = {}

local function CreateMobileButton(index)
    local column = (index - 1) - math.floor((index - 1) / GRID_COLUMNS) * GRID_COLUMNS
    local row = math.floor((index - 1) / GRID_COLUMNS)

    -- The unscaled slot owns layout spacing. Scaling a button anchored directly to the bar also
    -- scales its anchor offsets in the 1.12 frame model, which makes gaps balloon with button size.
    local slot = CreateFrame(
        "Frame",
        "BenillaMobileSlot" .. index,
        bar
    )
    slot:SetWidth(ButtonSize())
    slot:SetHeight(ButtonSize())
    slot:SetPoint(
        "BOTTOMLEFT",
        bar,
        "BOTTOMLEFT",
        column * (ButtonSize() + BUTTON_SPACING),
        row * (ButtonSize() + BUTTON_SPACING)
    )

    local button = CreateFrame(
        "CheckButton",
        "BenillaMobileButton" .. index,
        slot,
        "ActionButtonTemplate"
    )
    button:SetWidth(BASE_BUTTON_SIZE)
    button:SetHeight(BASE_BUTTON_SIZE)
    button:SetScale(ButtonSize() / BASE_BUTTON_SIZE)
    button:SetPoint("CENTER", slot, "CENTER", 0, 0)
    button.mobileSlot = slot

    button.actionIndex = index
    button.actionSlot = ActionSlotForButton(index)
    button:RegisterForClicks("LeftButtonUp", "RightButtonUp")
    button:RegisterForDrag("LeftButton", "RightButton")

    local hotkey = GetButtonPart(button, "HotKey")
    if hotkey then
        hotkey:SetText("")
        hotkey:Hide()
    end

    -- The buttons are ordinary action-slot views, not a second spell system.
    button:SetScript("OnClick", function()
        local action = this.actionSlot
        if not action then
            return
        end

        if arg1 == "RightButton" then
            UseAction(action, 0, 1)
        else
            UseAction(action, 0, 0)
        end
    end)

    button:SetScript("OnDragStart", function()
        local action = this.actionSlot
        if action then
            PickupAction(action)
        end
    end)

    button:SetScript("OnReceiveDrag", function()
        local action = this.actionSlot
        if action then
            PlaceAction(action)
        end
    end)

    -- Keep the mask in the unscaled slot's coordinate space. The ActionButton scale is for
    -- its artwork and hit target; inheriting it on this Model compounds the transform and can
    -- shift the MDX cooldown mesh away from the enlarged icon.
    local cooldown = CreateFrame("Model", nil, slot)
    cooldown:SetWidth(ButtonSize())
    cooldown:SetHeight(ButtonSize())
    cooldown:SetPoint("CENTER", slot, "CENTER", 0, 0)
    cooldown:SetModel("Interface\\Cooldown\\UI-Cooldown-Indicator.mdx")
    cooldown:SetFrameStrata("HIGH")
    cooldown:SetFrameLevel(slot:GetFrameLevel() + 10)
    cooldown:SetScript(
        "OnUpdateModel",
        BenillaMobile_CooldownFrame_OnUpdateModel
    )
    cooldown:SetScript(
        "OnAnimFinished",
        BenillaMobile_CooldownFrame_OnAnimFinished
    )
    cooldown:Hide()
    button.mobileCooldown = cooldown

    local cooldownText = button:CreateFontString(nil, "OVERLAY")
    cooldownText:SetFont("Fonts\\FRIZQT__.TTF", 15, "OUTLINE")
    cooldownText:SetPoint("CENTER", button, "CENTER", 0, 0)
    cooldownText:SetText("")
    cooldownText:Hide()
    button.mobileCooldownText = cooldownText

    return button
end

local function UpdateCooldown(button, action)
    local start, duration, enable = GetActionCooldown(action)

    if start and duration and enable
        and start > 0 and duration > 0 and enable > 0 then
        BenillaMobile_CooldownFrame_SetTimer(
            button.mobileCooldown, start, duration, enable
        )

        local remaining = duration - (GetTime() - start)
        if remaining < 0 then
            remaining = 0
        end

        if remaining >= 10 then
            button.mobileCooldownText:SetText(
                tostring(math.floor(remaining))
            )
        elseif remaining > 0 then
            local tenths = math.floor(remaining * 10)
            local whole = math.floor(tenths / 10)
            local decimal = tenths - whole * 10
            button.mobileCooldownText:SetText(
                tostring(whole) .. "." .. tostring(decimal)
            )
        else
            button.mobileCooldownText:SetText("")
        end

        button.mobileCooldownText:Show()
    else
        button.mobileCooldown:Hide()
        button.mobileCooldownText:SetText("")
        button.mobileCooldownText:Hide()
    end
end

local function UpdateMobileButton(button)
    local action = ActionSlotForButton(button.actionIndex)
    button.actionSlot = action

    local icon = GetButtonPart(button, "Icon")
    local count = GetButtonPart(button, "Count")

    if not HasAction(action) then
        if icon then
            icon:SetTexture(nil)
            icon:SetVertexColor(1.0, 1.0, 1.0)
            icon:Show()
        end
        if count then
            count:SetText("")
            count:Hide()
        end
        button:SetChecked(0)
        button.mobileCooldown:Hide()
        button.mobileCooldownText:SetText("")
        button.mobileCooldownText:Hide()
        return
    end

    local texture = GetActionTexture(action)
    if icon then
        icon:SetTexture(texture)
        icon:Show()

        local usable, noMana = IsUsableAction(action)
        local consumable = IsConsumableAction(action)
        local actionCount = GetActionCount(action)

        if consumable and actionCount == 0 then
            icon:SetVertexColor(0.45, 0.45, 0.45)
        elseif usable then
            if IsActionInRange(action) == 0 then
                icon:SetVertexColor(
                    RANGE_COLOR_R, RANGE_COLOR_G, RANGE_COLOR_B
                )
            else
                icon:SetVertexColor(1.0, 1.0, 1.0)
            end
        elseif noMana then
            icon:SetVertexColor(0.5, 0.5, 1.0)
        else
            icon:SetVertexColor(0.4, 0.4, 0.4)
        end
    end

    if count then
        if IsConsumableAction(action) then
            local amount = GetActionCount(action)
            count:SetText(amount > 0 and tostring(amount) or "")
            count:Show()
        else
            count:SetText("")
            count:Hide()
        end
    end

    button:SetChecked(
        (IsCurrentAction(action) or IsAutoRepeatAction(action)) and 1 or 0
    )

    UpdateCooldown(button, action)
end

local function UpdateAllButtons()
    local i = 1
    while i <= BUTTON_COUNT do
        UpdateMobileButton(mobileButtons[i])
        i = i + 1
    end

    pageLabel:SetText(
        tostring(BenillaMobileDB.page) .. " / " .. tostring(PAGE_COUNT)
    )

    previousPageButton:SetAlpha(BenillaMobileDB.page > 1 and 1.0 or 0.45)
    nextPageButton:SetAlpha(BenillaMobileDB.page < PAGE_COUNT and 1.0 or 0.45)
end

local function SetPage(page)
    BenillaMobileDB.page = ClampPage(page)
    UpdateAllButtons()
end

previousPageButton:SetScript("OnClick", function()
    SetPage(BenillaMobileDB.page - 1)
end)

nextPageButton:SetScript("OnClick", function()
    SetPage(BenillaMobileDB.page + 1)
end)

local index = 1
while index <= BUTTON_COUNT do
    mobileButtons[index] = CreateMobileButton(index)
    index = index + 1
end

local function ResizeButtons()
    local size = ButtonSize()
    BAR_WIDTH = GRID_COLUMNS * size + (GRID_COLUMNS - 1) * BUTTON_SPACING
    BAR_HEIGHT = GRID_ROWS * size + (GRID_ROWS - 1) * BUTTON_SPACING + HEADER_HEIGHT

    bar:SetWidth(BAR_WIDTH)
    bar:SetHeight(BAR_HEIGHT)
    header:SetWidth(BAR_WIDTH)

    local i = 1
    while i <= BUTTON_COUNT do
        local button = mobileButtons[i]
        local slot = button.mobileSlot
        local column = (i - 1) - math.floor((i - 1) / GRID_COLUMNS) * GRID_COLUMNS
        local row = math.floor((i - 1) / GRID_COLUMNS)

        -- Slot placement stays unscaled; only the button's visuals/hit target grow.
        slot:SetWidth(size)
        slot:SetHeight(size)
        slot:ClearAllPoints()
        slot:SetPoint(
            "BOTTOMLEFT",
            bar,
            "BOTTOMLEFT",
            column * (size + BUTTON_SPACING),
            row * (size + BUTTON_SPACING)
        )

        button:SetWidth(BASE_BUTTON_SIZE)
        button:SetHeight(BASE_BUTTON_SIZE)
        button:SetScale(size / BASE_BUTTON_SIZE)
        button:ClearAllPoints()
        button:SetPoint("CENTER", slot, "CENTER", 0, 0)

        -- The cooldown Model is a sibling of the scaled button, so size its viewport to the
        -- slot and center it there. This keeps its fixed MDX mesh aligned as button size changes.
        button.mobileCooldown:SetWidth(size)
        button.mobileCooldown:SetHeight(size)
        button.mobileCooldown:ClearAllPoints()
        button.mobileCooldown:SetPoint("CENTER", slot, "CENTER", 0, 0)

        button.mobileCooldownText:SetFont(
            "Fonts\\FRIZQT__.TTF", 15, "OUTLINE"
        )
        i = i + 1
    end

    UpdateAllButtons()
end

local function PositionBar()
    bar:ClearAllPoints()
    bar:SetPoint(
        "BOTTOM",
        UIParent,
        "BOTTOM",
        BenillaMobileDB.x,
        BenillaMobileDB.y
    )
end

local function IsPhysicalGamepadConnected()
    if type(Gamepad_IsConnected) == "function" then
        return Gamepad_IsConnected()
    end
    return false
end

local function ShouldShowBar()
    if BenillaMobileDB.visibility == "show" then
        return true
    elseif BenillaMobileDB.visibility == "hide" then
        return false
    end

    if BenillaMobileDB.autoHideWithGamepad
        and IsPhysicalGamepadConnected() then
        return false
    end

    return true
end

local elapsedSinceUpdate = 0
local shown = nil

local updateFrame = CreateFrame("Frame", "BenillaMobileUpdateFrame", UIParent)
updateFrame:SetScript("OnUpdate", function()
    local shouldShow = ShouldShowBar()

    if shown ~= shouldShow then
        shown = shouldShow
        if shouldShow then
            bar:Show()
            header:Show()
            UpdateAllButtons()
        else
            bar:Hide()
        end
    end

    local elapsed = arg1
    if not elapsed then
        return
    end

    elapsedSinceUpdate = elapsedSinceUpdate + elapsed
    if elapsedSinceUpdate < UPDATE_INTERVAL then
        return
    end
    elapsedSinceUpdate = 0

    if bar:IsShown() then
        UpdateAllButtons()
    end
end)

local function NudgeBar(dx, dy)
    BenillaMobileDB.x = BenillaMobileDB.x + dx
    BenillaMobileDB.y = BenillaMobileDB.y + dy
    PositionBar()
end

SLASH_BENILLAMOBILE1 = "/bm"
SlashCmdList["BENILLAMOBILE"] = function(message)
    -- Vanilla's Lua dialect has string.find, but not Lua 5.1's string.match.
    local _, _, command, value =
        string.find(message or "", "^%s*(%S+)%s*(.-)%s*$")
    command = string.lower(command or "")
    value = value or ""

    if command == "next" then
        SetPage(BenillaMobileDB.page + 1)
    elseif command == "prev" or command == "previous" then
        SetPage(BenillaMobileDB.page - 1)
    elseif command == "page" then
        SetPage(value)
    elseif command == "show" then
        BenillaMobileDB.visibility = "show"
        bar:Show()
    elseif command == "hide" then
        BenillaMobileDB.visibility = "hide"
        bar:Hide()
    elseif command == "auto" then
        BenillaMobileDB.visibility = "auto"
    elseif command == "left" then
        NudgeBar(-10, 0)
    elseif command == "right" then
        NudgeBar(10, 0)
    elseif command == "up" then
        NudgeBar(0, 10)
    elseif command == "down" then
        NudgeBar(0, -10)
    elseif command == "size" then
        local size = tonumber(value)
        if size then
            BenillaMobileDB.buttonSize =
                math.max(40, math.min(MAX_BUTTON_SIZE, size))
            ResizeButtons()
            PositionBar()
        end
    elseif command == "gamepad" then
        if value == "on" then
            BenillaMobileDB.autoHideWithGamepad = true
        elseif value == "off" then
            BenillaMobileDB.autoHideWithGamepad = false
        end
    elseif command == "reset" then
        BenillaMobileDB.x = DEFAULT_X
        BenillaMobileDB.y = DEFAULT_Y
        BenillaMobileDB.buttonSize = DEFAULT_BUTTON_SIZE
        BenillaMobileDB.visibility = "auto"
        BenillaMobileDB.autoHideWithGamepad = true
        SetPage(1)
        ResizeButtons()
        PositionBar()
    else
        DEFAULT_CHAT_FRAME:AddMessage(
            "BenillaMobile: /bm next | prev | page <1-" ..
            tostring(PAGE_COUNT) ..
            "> | show | hide | auto | left/right/up/down | size <40-96> | gamepad on/off | reset"
        )
    end
end

-- Respect SavedVariables and establish the configured layout at load.
PositionBar()
SetPage(BenillaMobileDB.page)
ResizeButtons()
bar:Show()
header:Show()
