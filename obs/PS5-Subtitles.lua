-- Keep this OBS entry point stable. The installer replaces the local overlay module.
local obs, ffi = obslua, require("ffi")
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
local max_bytes=256*1024
local invalid=ffi.cast("void*", -1)
local bytes=ffi.new("char[?]", max_bytes+1)
local size=ffi.new("unsigned long[1]")
local function read(path, limit)
    assert(limit>=0 and limit<=max_bytes, "Invalid SubZir read length")
    local h=ffi.C.CreateFileW(path,2147483648,7,nil,3,128,nil)
    if h==invalid then return nil end
    local ok=ffi.C.ReadFile(h,bytes,limit+1,size,nil)
    ffi.C.CloseHandle(h)
    if ok~=0 and size[0]<=limit then return ffi.string(bytes,size[0]) end
end
local function write(path, data)
    local h=ffi.C.CreateFileW(path,1073741824,7,nil,2,128,nil)
    if h==invalid then return false end
    local ok=ffi.C.WriteFile(h,data,#data,size,nil)
    ffi.C.CloseHandle(h)
    return ok~=0 and size[0]==#data
end

local root=script_path()
local module_path=wide(root.."SubZir-overlay.lua")
local backup_path=wide(root.."SubZir-overlay.previous.lua")
local request=wide(root.."../runtime/obs-module-check.txt")
local response=wide(root.."../runtime/obs-module-ack.txt")
local host={obs=obs, root=root, path=wide, read=read, write=write}
local active, previous, seen, last_reply
local function load_module(source)
    local chunk,err=loadstring(source,"@SubZir-overlay.lua")
    assert(chunk,err)
    -- OBS rebinds the global error() to its logger. Modules need throwing
    -- errors so pcall can discard a failed candidate and restore the renderer.
    setfenv(chunk,setmetatable({error=function(reason) assert(false,reason) end},{__index=getfenv(1)}))
    local module=chunk(host)
    for _,name in ipairs({"tick","scene_changed","description","button_label","add"}) do
        assert(type(module)=="table" and type(module[name])=="function","Missing overlay callback: "..name)
    end
    module.tick(0)
    return {source=source, module=module}
end
local function warn(reason)
    obs.script_log(obs.LOG_WARNING,"SubZir overlay: "..tostring(reason))
end
local function activate(source)
    local ok, candidate=pcall(load_module,source)
    if ok then
        if active and active.module.unload then
            pcall(active.module.unload)
            pcall(candidate.module.scene_changed)
        end
        previous,active=active,candidate
        return true
    end
    warn(candidate)
    return false
end
local function dispatch(name,...)
    if not active then return end
    local ok,value=pcall(active.module[name],...)
    if ok then return value end
    warn(value)
    if active.module.unload then pcall(active.module.unload) end
    active,previous=previous,nil
    if active then
        -- Force the restored renderer to refresh the current scene.
        pcall(active.module.scene_changed)
    end
end
local function refresh()
    local source=read(module_path,max_bytes)
    if source~=seen then
        seen=source
        if source and (not active or active.source~=source) then activate(source) end
    end
    local nonce=read(request,128)
    if nonce and #nonce==32 and nonce:match("^%x+$") then
        local state=active and (active.source==source and "ready" or "fallback") or "failed"
        local reply=nonce.." "..state
        if reply~=last_reply and write(response,reply) then last_reply=reply end
    end
end

local function scene_event(event)
    if event==obs.OBS_FRONTEND_EVENT_SCENE_CHANGED then dispatch("scene_changed") end
end
function script_load(settings)
    local backup=read(backup_path,max_bytes)
    if backup then activate(backup) end
    refresh()
    obs.obs_frontend_add_event_callback(scene_event)
    obs.timer_add(refresh,1000)
end
function script_unload()
    if active and active.module.unload then pcall(active.module.unload) end
    obs.timer_remove(refresh)
    obs.obs_frontend_remove_event_callback(scene_event)
end
function script_tick(seconds) dispatch("tick",seconds) end
function script_description()
    return dispatch("description") or "SubZir · game subtitles"
end
local function add_text() return dispatch("add") or false end
function script_properties()
    local props=obs.obs_properties_create()
    obs.obs_properties_add_button(props,"add",dispatch("button_label") or "Add subtitles to the current scene",add_text)
    return props
end
