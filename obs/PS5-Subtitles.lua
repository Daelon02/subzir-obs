obs = obslua
local title = "PS5 UA · QuickMT"
local folder = script_path() .. "../runtime/"
local last, pending = nil, nil

function script_description()
    return "Виберіть у OBS сцену з грою, потім натисніть кнопку нижче. Після цього поверніться до програми перекладу."
end

local function add_text()
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
    last = nil
    return false
end
function script_properties()
    local props = obs.obs_properties_create()
    obs.obs_properties_add_button(props, "add", "Додати субтитри до поточної сцени", add_text)
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
function script_tick(seconds)
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
    local src=obs.obs_get_source_by_name(title)
    if not src then return end
    local cfg=obs.obs_data_create()
    obs.obs_data_set_string(cfg,"text",text)
    obs.obs_source_update(src,cfg)
    if obs.obs_source_showing(src) then pending=id end
    obs.obs_data_release(cfg)
    obs.obs_source_release(src)
    last=id
end
