-- OBS callbacks are owned by the loader; this module only handles the overlay.
local host = ...
local obs = host.obs
local title = "SubZir · Subtitles"
local legacy = "PS5 UA · QuickMT"
local function source_title()
    local src = obs.obs_get_source_by_name(legacy)
    if src then obs.obs_source_release(src); return legacy end
    return title
end
local function message(key, fallback)
    local prefs = obs.obs_data_create_from_json_file(host.root .. "../runtime/languages.json")
    local code = prefs and obs.obs_data_get_string(prefs, "ui") or ""
    if prefs then obs.obs_data_release(prefs) end
    if not code:match("^[a-z][a-z][a-z]?$") then code = (obs.obs_get_locale() or "en"):sub(1,2):lower() end
    local data = obs.obs_data_create_from_json_file(host.root .. "../app/locales/" .. code .. ".json")
    if not data then return fallback end
    local value = obs.obs_data_get_string(data, key)
    obs.obs_data_release(data)
    return value ~= "" and value or fallback
end
local folder = host.root .. "../runtime/"
local scene_file = host.path(folder.."hud-scene.txt")
local function remember_scene()
    local scene = obs.obs_frontend_get_current_scene()
    if scene then
        local name = obs.obs_source_get_name(scene)
        if name and name ~= "" then host.write(scene_file, name) end
        obs.obs_source_release(scene)
    end
end
remember_scene()
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
-- This private scene shares the game transform (including cropping) with the PNG overlay.
local layer_title="SubZir · Zone text"
local capability=host.path(folder.."obs-layers-v1.txt")
local layer_files={host.path(folder.."layers-0.json"),host.path(folder.."layers-1.json")}
local heartbeat=4
local native_ok=true
local settings_cache={}
local expected_count=0
local owner_key="subzir.zone-text.v1"
local function owned_layer(item)
    if not item then return false end
    local private=obs.obs_source_get_private_settings(obs.obs_sceneitem_get_source(item))
    local ours=obs.obs_data_get_bool(private,owner_key)
    obs.obs_data_release(private)
    return ours
end
local function drop_layers(scene)
    local item=obs.obs_scene_find_source(scene,layer_title)
    if owned_layer(item) then obs.obs_sceneitem_remove(item) end
    settings_cache={}
end
local function layer_scene(scene)
    local item=obs.obs_scene_find_source(scene,layer_title)
    if item and not owned_layer(item) then return nil end
    if not item then
        local nested=obs.obs_scene_create_private(layer_title)
        if not nested then return nil end
        local source=obs.obs_scene_get_source(nested)
        local private=obs.obs_source_get_private_settings(source)
        obs.obs_data_set_bool(private,owner_key,true);obs.obs_data_release(private)
        local cfg=obs.obs_source_get_settings(source)
        obs.obs_data_set_bool(cfg,"custom_size",true)
        obs.obs_data_set_int(cfg,"cx",1280);obs.obs_data_set_int(cfg,"cy",720)
        obs.obs_data_release(cfg)
        obs.obs_source_load(source)
        item=obs.obs_scene_add(scene,source)
        if item then
            local siblings=obs.obs_scene_enum_items(scene)
            for i,sibling in ipairs(siblings or {}) do
                if obs.obs_source_get_name(obs.obs_sceneitem_get_source(sibling))==zone_title then
                    obs.obs_sceneitem_set_order_position(item,i)
                    break
                end
            end
            if siblings then obs.sceneitem_list_release(siblings) end
        end
        obs.obs_scene_release(nested)
        settings_cache={}
    end
    if item then return obs.obs_scene_from_source(obs.obs_sceneitem_get_source(item)),item end
end
local function put(scene,name,kind,cfg,signature,x,y,w,h,fit)
    local item=obs.obs_scene_find_source(scene,name)
    if item and obs.obs_source_get_id(obs.obs_sceneitem_get_source(item))~=kind then
        obs.obs_sceneitem_remove(item);item=nil
    end
    if not item then
        local src=obs.obs_source_create_private(kind,name,cfg)
        if not src then return false end
        item=obs.obs_scene_add(scene,src)
        obs.obs_source_release(src)
        if not item then return false end
    elseif settings_cache[name]~=signature then
        obs.obs_source_update(obs.obs_sceneitem_get_source(item),cfg)
    end
    settings_cache[name]=signature
    local info=obs.obs_transform_info()
    info.pos.x=x;info.pos.y=y;info.scale.x=1;info.scale.y=1;info.alignment=5
    info.bounds_type=fit and obs.OBS_BOUNDS_MAX_ONLY or obs.OBS_BOUNDS_STRETCH
    info.bounds.x=w;info.bounds.y=h;info.bounds_alignment=0
    obs.obs_sceneitem_set_info2(item,info)
    obs.obs_sceneitem_set_visible(item,true)
    return true
end
local function native_row(scene,row,index)
    local function num(k) return obs.obs_data_get_int(row,k) end
    local bgname="SubZir background "..index
    local image=obs.obs_data_get_string(row,"image")
    if num("opacity")>0 then
        local bg=obs.obs_data_create()
        local kind
        if image~="" then
            if not image:match("^zone%-back%-%d+%-[01]%.png$") then obs.obs_data_release(bg);return false end
            kind="image_source"
            obs.obs_data_set_string(bg,"file",folder..image)
        else
            kind="color_source_v3"
            obs.obs_data_set_int(bg,"width",2);obs.obs_data_set_int(bg,"height",2)
            obs.obs_data_set_int(bg,"color",num("background"))
        end
        local signature=obs.obs_data_get_json(bg)..obs.obs_data_get_string(row,"revision")
        local ok=put(scene,bgname,kind,bg,signature,num("bx"),num("by"),num("bw"),num("bh"),false)
        obs.obs_data_release(bg)
        if not ok then return false end
    else
        local item=obs.obs_scene_find_source(scene,bgname)
        if item then obs.obs_sceneitem_set_visible(item,false) end
    end
    local text=obs.obs_data_get_obj(row,"cfg")
    if not text then return false end
    local name="SubZir text "..index
    local ok=put(scene,name,"text_gdiplus_v2",text,obs.obs_data_get_json(text),
        num("x"),num("y"),num("w"),num("h"),true)
    obs.obs_data_release(text)
    -- When a row gains a background it must stay behind its own text.
    if ok then
        local bg=obs.obs_scene_find_source(scene,bgname)
        if bg then obs.obs_sceneitem_set_order(bg,obs.OBS_ORDER_MOVE_TOP) end
        obs.obs_sceneitem_set_order(obs.obs_scene_find_source(scene,name),obs.OBS_ORDER_MOVE_TOP)
    end
    return ok
end
local function paint_layers(current,scene,data)
    local nested,layer=layer_scene(scene)
    if not nested then return false end
    local src=obs.obs_scene_get_source(nested)
    local w,h=obs.obs_source_get_width(src),obs.obs_source_get_height(src)
    if w~=1280 or h~=720 then return false end
    local rows=obs.obs_data_get_array(data,"layers")
    if not rows then return false end
    local count=obs.obs_data_array_count(rows)
    local ok=true
    for i=1,count do
        local row=obs.obs_data_array_item(rows,i-1)
        ok=native_row(nested,row,i)
        obs.obs_data_release(row)
        if not ok then break end
    end
    obs.obs_data_array_release(rows)
    if not ok then return false end
    -- Also prune rows left by a previous module instance after a hot reload.
    local items=obs.obs_scene_enum_items(nested)
    for _,item in ipairs(items or {}) do
        local name=obs.obs_source_get_name(obs.obs_sceneitem_get_source(item))
        local ordinal=tonumber(name:match(" (%d+)$"))
        if ordinal and ordinal>count then obs.obs_sceneitem_remove(item);settings_cache[name]=nil end
    end
    if items then obs.sceneitem_list_release(items) end
    place_layer(scene,current,layer)
    obs.obs_sceneitem_set_visible(layer,true)
    expected_count=count
    return true
end
local function apply_layers(id,slot)
    local raw=host.read(layer_files[tonumber(slot)+1],256*1024)
    if not raw then return false end
    local data=obs.obs_data_create_from_json(raw)
    if not data then return false end
    local current=obs.obs_frontend_get_current_scene()
    local scene=current and obs.obs_scene_from_source(current)
    local attached=scene and obs.obs_scene_find_source(scene,source_title())
    local ok=false
    if attached and obs.obs_data_get_string(data,"id")==id then
        local applied,result=pcall(paint_layers,current,scene,data)
        ok=applied and result
        if not applied then obs.script_log(obs.LOG_WARNING,"SubZir text layers: "..tostring(result)) end
        if ok then
            local img=obs.obs_scene_find_source(scene,zone_title)
            if img then obs.obs_sceneitem_set_visible(img,false) end
        else
            drop_layers(scene);native_ok=false;host.write(capability,"png")
        end
    end
    if current then obs.obs_source_release(current) end
    obs.obs_data_release(data)
    return ok
end
local function layers_visible()
    local current=obs.obs_frontend_get_current_scene()
    if not current then return false end
    local scene=obs.obs_scene_from_source(current)
    local item=scene and obs.obs_scene_find_source(scene,layer_title)
    local nested=item and obs.obs_scene_from_source(obs.obs_sceneitem_get_source(item))
    local ok=nested~=nil and obs.obs_sceneitem_visible(item)
    if ok then
        for i=1,expected_count do
            local field=obs.obs_scene_find_source(nested,"SubZir text "..i)
            local src=field and obs.obs_sceneitem_get_source(field)
            if not src or not obs.obs_source_showing(src) or obs.obs_source_get_width(src)==0 then ok=false;break end
        end
    end
    obs.obs_source_release(current)
    return ok
end

local function zones_frame(slot)
    local current=obs.obs_frontend_get_current_scene()
    if not current then return false end
    local scene=obs.obs_scene_from_source(current)
    if not scene or not obs.obs_scene_find_source(scene,source_title()) then obs.obs_source_release(current);return false end
    local item=obs.obs_scene_find_source(scene,zone_title)
    drop_layers(scene)
    local img=obs.obs_get_source_by_name(zone_title)
    if slot then
        local cfg=obs.obs_data_create()
        if not img then img=obs.obs_source_create("image_source",zone_title,cfg,nil) end
        if img then
            if not item then item=obs.obs_scene_add(scene,img) end
            local name="zones-"..slot..".png"
            local path=folder..name
            -- OBS may skip a marker and see the same slot twice. Give the image
            -- source a changed, valid path even then, without clearing its file.
            local settings=obs.obs_source_get_settings(img)
            if obs.obs_data_get_string(settings,"file")==path then path=folder.."./"..name end
            obs.obs_data_release(settings)
            obs.obs_data_set_string(cfg,"file",path);obs.obs_source_update(img,cfg)
            if item then place_layer(scene,current,item);obs.obs_sceneitem_set_visible(item,true) end
        end
        obs.obs_data_release(cfg)
    elseif item then obs.obs_sceneitem_set_visible(item,false) end
    if img then obs.obs_source_release(img) end
    obs.obs_source_release(current)
    return slot==nil or item~=nil
end
local applied_style, style_elapsed = nil, 0

local function description()
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
local caption = host.path(folder.."caption.txt")
local ack = host.path(folder.."ack.txt")
local style_file = host.path(folder.."subtitle-style.json")
local function update_style()
    local raw = host.read(style_file, 32768)
    if not raw then return end
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
        obs.obs_data_set_int(cfg,"outline_color",obs.obs_data_get_int(settings,"outline_color"))
        local radius=obs.obs_data_get_int(settings,"outline_size")
        obs.obs_data_set_int(cfg,"outline_size",radius>0 and 2*math.min(radius,8) or 3)
        obs.obs_source_update(src,cfg)
        obs.obs_data_release(font)
        obs.obs_data_release(cfg)
        obs.obs_data_release(settings)
        applied_style=raw
    end
    obs.obs_source_release(src)
end
local function tick(seconds)
    heartbeat=heartbeat+seconds
    if heartbeat>=1 then heartbeat=0;host.write(capability,native_ok and "ready" or "png") end
    style_elapsed=style_elapsed+seconds
    if style_elapsed>=0.4 then
        style_elapsed=0
        update_style()
    end
    if pending then
        if (not pending:match("^layers%-") or layers_visible()) and host.write(ack, pending) then pending=nil end
    end
    local raw=host.read(caption, 32768)
    if not raw then return end
    local id,text=raw:match("^([^\n]+)\n(.*)$")
    if not id or id==last then return end
    local slot=id:match("^layers%-([01])%-")
    if slot then
        if not native_ok or not apply_layers(id,slot) then return end
    elseif not zones_frame(id:match("^zones%-([01])%-")) then return end
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

local function clear_layers()
    local scenes=obs.obs_frontend_get_scenes()
    for _,current in ipairs(scenes or {}) do
        local scene=obs.obs_scene_from_source(current)
        if scene then drop_layers(scene) end
    end
    if scenes then obs.source_list_release(scenes) end
    pending=nil;last=nil
end
return {
    unload = function() clear_layers();host.write(capability,"png") end,
    tick = tick,
    scene_changed = function() clear_layers();remember_scene() end,
    description = description,
    button_label = function() return message("AddScene", "Add subtitles to the current scene") end,
    add = add_text
}
