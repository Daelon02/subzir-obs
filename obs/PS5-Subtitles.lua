obs = obslua
local title = "SubZir · Subtitles"
local legacy = "PS5 UA · QuickMT"
local function source_title()
    local src = obs.obs_get_source_by_name(legacy)
    if src then obs.obs_source_release(src); return legacy end
    return title
end
local function message(key, fallback)
    local prefs = obs.obs_data_create_from_json_file(script_path() .. "../runtime/languages.json")
    local code = prefs and obs.obs_data_get_string(prefs, "ui") or ""
    if prefs then obs.obs_data_release(prefs) end
    if not code:match("^[a-z][a-z][a-z]?$") then code = (obs.obs_get_locale() or "en"):sub(1,2):lower() end
    local data = obs.obs_data_create_from_json_file(script_path() .. "../app/locales/" .. code .. ".json")
    if not data then return fallback end
    local value = obs.obs_data_get_string(data, key)
    obs.obs_data_release(data)
    return value ~= "" and value or fallback
end
local folder = script_path() .. "../runtime/"
local last, pending = nil, nil
local zone_title = "SubZir · Zones"
local game_types = {game_capture=true, dshow_input=true, ffmpeg_source=true, window_capture=true, monitor_capture=true, vlc_source=true}
local function place_layer(scene, current, layer)
    local items = obs.obs_scene_enum_items(scene)
    local game, count = nil, 0
    for _,item in ipairs(items or {}) do
        local source = obs.obs_sceneitem_get_source(item)
        if obs.obs_sceneitem_visible(item) and game_types[obs.obs_source_get_unversioned_id(source)] then
            game=item; count=count+1
        end
    end
    local bounds=obs.vec2()
    bounds.x=obs.obs_source_get_width(current); bounds.y=obs.obs_source_get_height(current)
    if count==1 then
        local source=obs.obs_sceneitem_get_source(game)
        local w,h=obs.obs_source_get_width(source),obs.obs_source_get_height(source)
        if w>0 and h>0 then
            local info=obs.obs_transform_info()
            obs.obs_sceneitem_get_info2(game,info)
            info.scale.x=info.scale.x*w/1280; info.scale.y=info.scale.y*h/720
            obs.obs_sceneitem_set_info2(layer,info)
            local crop=obs.obs_sceneitem_crop()
            obs.obs_sceneitem_get_crop(game,crop)
            crop.left=math.floor(crop.left*1280/w); crop.right=math.floor(crop.right*1280/w)
            crop.top=math.floor(crop.top*720/h); crop.bottom=math.floor(crop.bottom*720/h)
            obs.obs_sceneitem_set_crop(layer,crop)
            if items then obs.sceneitem_list_release(items) end
            return
        end
    end
    if items then obs.sceneitem_list_release(items) end
    local info=obs.obs_transform_info()
    info.scale.x=1; info.scale.y=1; info.alignment=5
    info.bounds_type=obs.OBS_BOUNDS_STRETCH; info.bounds.x=bounds.x; info.bounds.y=bounds.y
    obs.obs_sceneitem_set_info2(layer,info);obs.obs_sceneitem_set_crop(layer,obs.obs_sceneitem_crop())
end
local function zones_frame(slot)
    local current=obs.obs_frontend_get_current_scene()
    if not current then return false end
    local scene=obs.obs_scene_from_source(current)
    if not scene or not obs.obs_scene_find_source(scene,source_title()) then obs.obs_source_release(current);return false end
    local item=obs.obs_scene_find_source(scene,zone_title)
    local img=obs.obs_get_source_by_name(zone_title)
    if slot then
        local cfg=obs.obs_data_create()
        if not img then img=obs.obs_source_create("image_source",zone_title,cfg,nil) end
        if img then
            if not item then item=obs.obs_scene_add(scene,img) end
            obs.obs_data_set_string(cfg,"file","");obs.obs_source_update(img,cfg)
            obs.obs_data_set_string(cfg,"file",folder.."zones-"..slot..".png");obs.obs_source_update(img,cfg)
            if item then place_layer(scene,current,item);obs.obs_sceneitem_set_visible(item,true) end
        end
        obs.obs_data_release(cfg)
    elseif item then obs.obs_sceneitem_set_visible(item,false) end
    if img then obs.obs_source_release(img) end
    obs.obs_source_release(current)
    return slot==nil or item~=nil
end
function script_load(settings)
    obs.obs_frontend_add_event_callback(function(event)
        if event==obs.OBS_FRONTEND_EVENT_SCENE_CHANGED then last=nil end
    end)
end
local applied_style, style_elapsed = nil, 0

function script_description()
    return message("ConnectHint", "Add the script to your game scene once.")
end

local function add_text()
    local title = source_title()
    local current = obs.obs_frontend_get_current_scene()
    if not current then return false end
    local scene = obs.obs_scene_from_source(current)
    if not scene then obs.obs_source_release(current); return false end
    if not obs.obs_scene_find_source(scene, title) then
        local src = obs.obs_get_source_by_name(title)
        if not src then
            local cfg, font = obs.obs_data_create(), obs.obs_data_create()
            local width = obs.obs_source_get_width(current)
            obs.obs_data_set_string(font, "face", "Segoe UI")
            obs.obs_data_set_int(font, "size", math.max(24, math.floor(width / 40)))
            obs.obs_data_set_obj(cfg, "font", font)
            obs.obs_data_set_bool(cfg, "outline", true)
            obs.obs_data_set_int(cfg, "outline_size", 3)
            obs.obs_data_set_bool(cfg, "extents", true)
            obs.obs_data_set_bool(cfg, "extents_wrap", true)
            obs.obs_data_set_int(cfg, "extents_cx", math.floor(width * 0.9))
            obs.obs_data_set_int(cfg, "extents_cy", math.max(80, math.floor(width / 10)))
            obs.obs_data_set_string(cfg, "align", "center")
            src = obs.obs_source_create("text_gdiplus", title, cfg, nil)
            obs.obs_data_release(font)
            obs.obs_data_release(cfg)
        end
        if src then
            local item = obs.obs_scene_add(scene, src)
            local pos = obs.vec2()
            pos.x = math.floor(obs.obs_source_get_width(current) * 0.05)
            pos.y = math.floor(obs.obs_source_get_height(current) * 0.74)
            obs.obs_sceneitem_set_pos(item, pos)
            obs.obs_source_release(src)
        end
    end
    obs.obs_source_release(current)
    last, applied_style = nil, nil
    return false
end
function script_properties()
    local props = obs.obs_properties_create()
    obs.obs_properties_add_button(props, "add", message("AddScene", "Add subtitles to the current scene"), add_text)
    return props
end

-- LuaJIT's io.open uses the Windows ANSI code page. These Win32 calls also
-- support folders named in Ukrainian regardless of the system code page.
local ffi = require("ffi")
ffi.cdef[[
int MultiByteToWideChar(unsigned int, unsigned long, const char*, int, unsigned short*, int);
void* CreateFileW(const unsigned short*, unsigned long, unsigned long, void*, unsigned long, unsigned long, void*);
int CloseHandle(void*);
int ReadFile(void*, void*, unsigned long, unsigned long*, void*);
int WriteFile(void*, const void*, unsigned long, unsigned long*, void*);
]]
local function wide(s)
    local n=ffi.C.MultiByteToWideChar(65001,0,s,-1,nil,0)
    local buf=ffi.new("unsigned short[?]",n)
    ffi.C.MultiByteToWideChar(65001,0,s,-1,buf,n)
    return buf
end
local caption, ack = wide(folder.."caption.txt"), wide(folder.."ack.txt")
local bad=ffi.cast("void*", -1)
local buf=ffi.new("char[32768]")
local size=ffi.new("unsigned long[1]")
local style_file = wide(folder.."subtitle-style.json")
local function update_style()
    local h=ffi.C.CreateFileW(style_file,2147483648,7,nil,3,128,nil)
    if h==bad then return end
    local ok=ffi.C.ReadFile(h,buf,32768,size,nil)
    ffi.C.CloseHandle(h)
    if ok==0 then return end
    local raw=ffi.string(buf,size[0])
    if raw==applied_style then return end
    local src=obs.obs_get_source_by_name(source_title())
    if not src then return end
    local settings=obs.obs_data_create_from_json(raw)
    if settings then
        local font, cfg=obs.obs_data_create(), obs.obs_data_create()
        obs.obs_data_set_string(font,"face",obs.obs_data_get_string(settings,"face"))
        obs.obs_data_set_int(font,"size",math.max(8,math.min(250,obs.obs_data_get_int(settings,"size"))))
        obs.obs_data_set_int(font,"flags",obs.obs_data_get_int(settings,"flags"))
        obs.obs_data_set_obj(cfg,"font",font)
        obs.obs_data_set_int(cfg,"color",obs.obs_data_get_int(settings,"color"))
        obs.obs_data_set_int(cfg,"bk_color",obs.obs_data_get_int(settings,"background"))
        obs.obs_data_set_int(cfg,"bk_opacity",math.max(0,math.min(100,obs.obs_data_get_int(settings,"opacity"))))
        obs.obs_data_set_bool(cfg,"outline",obs.obs_data_get_bool(settings,"outline"))
        obs.obs_source_update(src,cfg)
        obs.obs_data_release(font)
        obs.obs_data_release(cfg)
        obs.obs_data_release(settings)
        applied_style=raw
    end
    obs.obs_source_release(src)
end
function script_tick(seconds)
    style_elapsed=style_elapsed+seconds
    if style_elapsed>=0.4 then
        style_elapsed=0
        update_style()
    end
    if pending then
        local h=ffi.C.CreateFileW(ack,1073741824,7,nil,2,128,nil)
        if h~=bad then
            ffi.C.WriteFile(h,pending,#pending,size,nil)
            ffi.C.CloseHandle(h)
        end
        pending=nil
    end
    local h=ffi.C.CreateFileW(caption,2147483648,7,nil,3,128,nil)
    if h==bad then return end
    local ok=ffi.C.ReadFile(h,buf,32768,size,nil)
    ffi.C.CloseHandle(h)
    if ok==0 then return end
    local id,text=ffi.string(buf,size[0]):match("^([^\n]+)\n(.*)$")
    if not id or id==last then return end
    if not zones_frame(id:match("^zones%-([01])%-")) then return end
    local src=obs.obs_get_source_by_name(source_title())
    if not src then return end
    local cfg=obs.obs_data_create()
    obs.obs_data_set_string(cfg,"text",text)
    obs.obs_source_update(src,cfg)
    if obs.obs_source_showing(src) then pending=id end
    obs.obs_data_release(cfg)
    obs.obs_source_release(src)
    last=id
end
