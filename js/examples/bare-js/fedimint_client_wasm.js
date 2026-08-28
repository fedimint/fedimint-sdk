let wasm

const heap = new Array(128).fill(undefined)

heap.push(undefined, null, true, false)

function getObject(idx) {
  return heap[idx]
}

let heap_next = heap.length

function dropObject(idx) {
  if (idx < 132) return
  heap[idx] = heap_next
  heap_next = idx
}

function takeObject(idx) {
  const ret = getObject(idx)
  dropObject(idx)
  return ret
}

function addHeapObject(obj) {
  if (heap_next === heap.length) heap.push(heap.length + 1)
  const idx = heap_next
  heap_next = heap[idx]

  heap[idx] = obj
  return idx
}

const cachedTextDecoder =
  typeof TextDecoder !== 'undefined'
    ? new TextDecoder('utf-8', { ignoreBOM: true, fatal: true })
    : {
        decode: () => {
          throw Error('TextDecoder not available')
        },
      }

if (typeof TextDecoder !== 'undefined') {
  cachedTextDecoder.decode()
}

let cachedUint8Memory0 = null

function getUint8Memory0() {
  if (cachedUint8Memory0 === null || cachedUint8Memory0.byteLength === 0) {
    cachedUint8Memory0 = new Uint8Array(wasm.memory.buffer)
  }
  return cachedUint8Memory0
}

function getStringFromWasm0(ptr, len) {
  ptr = ptr >>> 0
  return cachedTextDecoder.decode(getUint8Memory0().subarray(ptr, ptr + len))
}

let WASM_VECTOR_LEN = 0

const cachedTextEncoder =
  typeof TextEncoder !== 'undefined'
    ? new TextEncoder('utf-8')
    : {
        encode: () => {
          throw Error('TextEncoder not available')
        },
      }

const encodeString =
  typeof cachedTextEncoder.encodeInto === 'function'
    ? function (arg, view) {
        return cachedTextEncoder.encodeInto(arg, view)
      }
    : function (arg, view) {
        const buf = cachedTextEncoder.encode(arg)
        view.set(buf)
        return {
          read: arg.length,
          written: buf.length,
        }
      }

function passStringToWasm0(arg, malloc, realloc) {
  if (realloc === undefined) {
    const buf = cachedTextEncoder.encode(arg)
    const ptr = malloc(buf.length, 1) >>> 0
    getUint8Memory0()
      .subarray(ptr, ptr + buf.length)
      .set(buf)
    WASM_VECTOR_LEN = buf.length
    return ptr
  }

  let len = arg.length
  let ptr = malloc(len, 1) >>> 0

  const mem = getUint8Memory0()

  let offset = 0

  for (; offset < len; offset++) {
    const code = arg.charCodeAt(offset)
    if (code > 0x7f) break
    mem[ptr + offset] = code
  }

  if (offset !== len) {
    if (offset !== 0) {
      arg = arg.slice(offset)
    }
    ptr = realloc(ptr, len, (len = offset + arg.length * 3), 1) >>> 0
    const view = getUint8Memory0().subarray(ptr + offset, ptr + len)
    const ret = encodeString(arg, view)

    offset += ret.written
    ptr = realloc(ptr, len, offset, 1) >>> 0
  }

  WASM_VECTOR_LEN = offset
  return ptr
}

function isLikeNone(x) {
  return x === undefined || x === null
}

let cachedInt32Memory0 = null

function getInt32Memory0() {
  if (cachedInt32Memory0 === null || cachedInt32Memory0.byteLength === 0) {
    cachedInt32Memory0 = new Int32Array(wasm.memory.buffer)
  }
  return cachedInt32Memory0
}

function debugString(val) {
  // primitive types
  const type = typeof val
  if (type == 'number' || type == 'boolean' || val == null) {
    return `${val}`
  }
  if (type == 'string') {
    return `"${val}"`
  }
  if (type == 'symbol') {
    const description = val.description
    if (description == null) {
      return 'Symbol'
    } else {
      return `Symbol(${description})`
    }
  }
  if (type == 'function') {
    const name = val.name
    if (typeof name == 'string' && name.length > 0) {
      return `Function(${name})`
    } else {
      return 'Function'
    }
  }
  // objects
  if (Array.isArray(val)) {
    const length = val.length
    let debug = '['
    if (length > 0) {
      debug += debugString(val[0])
    }
    for (let i = 1; i < length; i++) {
      debug += ', ' + debugString(val[i])
    }
    debug += ']'
    return debug
  }
  // Test for built-in
  const builtInMatches = /\[object ([^\]]+)\]/.exec(toString.call(val))
  let className
  if (builtInMatches.length > 1) {
    className = builtInMatches[1]
  } else {
    // Failed to match the standard '[object ClassName]'
    return toString.call(val)
  }
  if (className == 'Object') {
    // we're a user defined class or Object
    // JSON.stringify avoids problems with cycles, and is generally much
    // easier than looping through ownProperties of `val`.
    try {
      return 'Object(' + JSON.stringify(val) + ')'
    } catch (_) {
      return 'Object'
    }
  }
  // errors
  if (val instanceof Error) {
    return `${val.name}: ${val.message}\n${val.stack}`
  }
  // TODO we could test for more things here, like `Set`s and `Map`s.
  return className
}

const CLOSURE_DTORS =
  typeof FinalizationRegistry === 'undefined'
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry((state) => {
        wasm.__wbindgen_export_2.get(state.dtor)(state.a, state.b)
      })

function makeMutClosure(arg0, arg1, dtor, f) {
  const state = { a: arg0, b: arg1, cnt: 1, dtor }
  const real = (...args) => {
    // First up with a closure we increment the internal reference
    // count. This ensures that the Rust closure environment won't
    // be deallocated while we're invoking it.
    state.cnt++
    const a = state.a
    state.a = 0
    try {
      return f(a, state.b, ...args)
    } finally {
      if (--state.cnt === 0) {
        wasm.__wbindgen_export_2.get(state.dtor)(a, state.b)
        CLOSURE_DTORS.unregister(state)
      } else {
        state.a = a
      }
    }
  }
  real.original = state
  CLOSURE_DTORS.register(real, state, state)
  return real
}
function __wbg_adapter_32(arg0, arg1, arg2) {
  try {
    const retptr = wasm.__wbindgen_add_to_stack_pointer(-16)
    wasm._dyn_core__ops__function__FnMut__A____Output___R_as_wasm_bindgen__closure__WasmClosure___describe__invoke__h8697e16cd1c64495(
      retptr,
      arg0,
      arg1,
      addHeapObject(arg2),
    )
    var r0 = getInt32Memory0()[retptr / 4 + 0]
    var r1 = getInt32Memory0()[retptr / 4 + 1]
    if (r1) {
      throw takeObject(r0)
    }
  } finally {
    wasm.__wbindgen_add_to_stack_pointer(16)
  }
}

function __wbg_adapter_35(arg0, arg1, arg2) {
  wasm._dyn_core__ops__function__FnMut__A____Output___R_as_wasm_bindgen__closure__WasmClosure___describe__invoke__h31a94cc9a05d5ca0(
    arg0,
    arg1,
    addHeapObject(arg2),
  )
}

function __wbg_adapter_42(arg0, arg1) {
  wasm._dyn_core__ops__function__FnMut_____Output___R_as_wasm_bindgen__closure__WasmClosure___describe__invoke__h4d11ec113460b95d(
    arg0,
    arg1,
  )
}

function __wbg_adapter_45(arg0, arg1, arg2) {
  try {
    const retptr = wasm.__wbindgen_add_to_stack_pointer(-16)
    wasm._dyn_core__ops__function__FnMut__A____Output___R_as_wasm_bindgen__closure__WasmClosure___describe__invoke__he7056307c6986185(
      retptr,
      arg0,
      arg1,
      addHeapObject(arg2),
    )
    var r0 = getInt32Memory0()[retptr / 4 + 0]
    var r1 = getInt32Memory0()[retptr / 4 + 1]
    if (r1) {
      throw takeObject(r0)
    }
  } finally {
    wasm.__wbindgen_add_to_stack_pointer(16)
  }
}

function __wbg_adapter_48(arg0, arg1) {
  wasm._dyn_core__ops__function__FnMut_____Output___R_as_wasm_bindgen__closure__WasmClosure___describe__invoke__h5b16159cdfa166b0(
    arg0,
    arg1,
  )
}

function __wbg_adapter_51(arg0, arg1, arg2) {
  wasm._dyn_core__ops__function__FnMut__A____Output___R_as_wasm_bindgen__closure__WasmClosure___describe__invoke__hebf1bd391d12b3cb(
    arg0,
    arg1,
    addHeapObject(arg2),
  )
}

function __wbg_adapter_54(arg0, arg1) {
  wasm._dyn_core__ops__function__FnMut_____Output___R_as_wasm_bindgen__closure__WasmClosure___describe__invoke__h09ee3e3abd173580(
    arg0,
    arg1,
  )
}

let stack_pointer = 128

function addBorrowedObject(obj) {
  if (stack_pointer == 1) throw new Error('out of js stack')
  heap[--stack_pointer] = obj
  return stack_pointer
}

function handleError(f, args) {
  try {
    return f.apply(this, args)
  } catch (e) {
    wasm.__wbindgen_exn_store(addHeapObject(e))
  }
}

function getArrayU8FromWasm0(ptr, len) {
  ptr = ptr >>> 0
  return getUint8Memory0().subarray(ptr / 1, ptr / 1 + len)
}
function __wbg_adapter_261(arg0, arg1, arg2, arg3) {
  wasm.wasm_bindgen__convert__closures__invoke2_mut__h8bdaa9faeb7d5075(
    arg0,
    arg1,
    addHeapObject(arg2),
    addHeapObject(arg3),
  )
}

const RpcHandleFinalization =
  typeof FinalizationRegistry === 'undefined'
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry((ptr) => wasm.__wbg_rpchandle_free(ptr >>> 0))
/**
 */
export class RpcHandle {
  static __wrap(ptr) {
    ptr = ptr >>> 0
    const obj = Object.create(RpcHandle.prototype)
    obj.__wbg_ptr = ptr
    RpcHandleFinalization.register(obj, obj.__wbg_ptr, obj)
    return obj
  }

  __destroy_into_raw() {
    const ptr = this.__wbg_ptr
    this.__wbg_ptr = 0
    RpcHandleFinalization.unregister(this)
    return ptr
  }

  free() {
    const ptr = this.__destroy_into_raw()
    wasm.__wbg_rpchandle_free(ptr)
  }
  /**
   */
  cancel() {
    wasm.rpchandle_cancel(this.__wbg_ptr)
  }
}

const WasmClientFinalization =
  typeof FinalizationRegistry === 'undefined'
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry((ptr) => wasm.__wbg_wasmclient_free(ptr >>> 0))
/**
 */
export class WasmClient {
  static __wrap(ptr) {
    ptr = ptr >>> 0
    const obj = Object.create(WasmClient.prototype)
    obj.__wbg_ptr = ptr
    WasmClientFinalization.register(obj, obj.__wbg_ptr, obj)
    return obj
  }

  __destroy_into_raw() {
    const ptr = this.__wbg_ptr
    this.__wbg_ptr = 0
    WasmClientFinalization.unregister(this)
    return ptr
  }

  free() {
    const ptr = this.__destroy_into_raw()
    wasm.__wbg_wasmclient_free(ptr)
  }
  /**
   * Open fedimint client with already joined federation.
   *
   * After you have joined a federation, you can reopen the fedimint client
   * with same client_name. Opening client with same name at same time is
   * not supported. You can close the current client by calling
   * `client.free()`. NOTE: The client will remain active until all the
   * running rpc calls have finished.
   * @param {string} client_name
   * @returns {Promise<WasmClient | undefined>}
   */
  static open(client_name) {
    const ptr0 = passStringToWasm0(
      client_name,
      wasm.__wbindgen_malloc,
      wasm.__wbindgen_realloc,
    )
    const len0 = WASM_VECTOR_LEN
    const ret = wasm.wasmclient_open(ptr0, len0)
    return takeObject(ret)
  }
  /**
   * Open a fedimint client by join a federation.
   * @param {string} client_name
   * @param {string} invite_code
   * @returns {Promise<WasmClient>}
   */
  static join_federation(client_name, invite_code) {
    const ptr0 = passStringToWasm0(
      client_name,
      wasm.__wbindgen_malloc,
      wasm.__wbindgen_realloc,
    )
    const len0 = WASM_VECTOR_LEN
    const ptr1 = passStringToWasm0(
      invite_code,
      wasm.__wbindgen_malloc,
      wasm.__wbindgen_realloc,
    )
    const len1 = WASM_VECTOR_LEN
    const ret = wasm.wasmclient_join_federation(ptr0, len0, ptr1, len1)
    return takeObject(ret)
  }
  /**
   * Call a fedimint client rpc the responses are returned using `cb`
   * callback. Each rpc call *can* return multiple responses by calling
   * `cb` multiple times. The returned RpcHandle can be used to cancel the
   * operation.
   * @param {string} module
   * @param {string} method
   * @param {string} payload
   * @param {Function} cb
   * @returns {RpcHandle}
   */
  rpc(module, method, payload, cb) {
    try {
      const ptr0 = passStringToWasm0(
        module,
        wasm.__wbindgen_malloc,
        wasm.__wbindgen_realloc,
      )
      const len0 = WASM_VECTOR_LEN
      const ptr1 = passStringToWasm0(
        method,
        wasm.__wbindgen_malloc,
        wasm.__wbindgen_realloc,
      )
      const len1 = WASM_VECTOR_LEN
      const ptr2 = passStringToWasm0(
        payload,
        wasm.__wbindgen_malloc,
        wasm.__wbindgen_realloc,
      )
      const len2 = WASM_VECTOR_LEN
      const ret = wasm.wasmclient_rpc(
        this.__wbg_ptr,
        ptr0,
        len0,
        ptr1,
        len1,
        ptr2,
        len2,
        addBorrowedObject(cb),
      )
      return RpcHandle.__wrap(ret)
    } finally {
      heap[stack_pointer++] = undefined
    }
  }
}

async function __wbg_load(module, imports) {
  if (typeof Response === 'function' && module instanceof Response) {
    if (typeof WebAssembly.instantiateStreaming === 'function') {
      try {
        return await WebAssembly.instantiateStreaming(module, imports)
      } catch (e) {
        if (module.headers.get('Content-Type') != 'application/wasm') {
          console.warn(
            '`WebAssembly.instantiateStreaming` failed because your server does not serve wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n',
            e,
          )
        } else {
          throw e
        }
      }
    }

    const bytes = await module.arrayBuffer()
    return await WebAssembly.instantiate(bytes, imports)
  } else {
    const instance = await WebAssembly.instantiate(module, imports)

    if (instance instanceof WebAssembly.Instance) {
      return { instance, module }
    } else {
      return instance
    }
  }
}

function __wbg_get_imports() {
  const imports = {}
  imports.wbg = {}
  imports.wbg.__wbg_wasmclient_new = function (arg0) {
    const ret = WasmClient.__wrap(arg0)
    return addHeapObject(ret)
  }
  imports.wbg.__wbindgen_object_drop_ref = function (arg0) {
    takeObject(arg0)
  }
  imports.wbg.__wbindgen_object_clone_ref = function (arg0) {
    const ret = getObject(arg0)
    return addHeapObject(ret)
  }
  imports.wbg.__wbindgen_string_new = function (arg0, arg1) {
    const ret = getStringFromWasm0(arg0, arg1)
    return addHeapObject(ret)
  }
  imports.wbg.__wbindgen_cb_drop = function (arg0) {
    const obj = takeObject(arg0).original
    if (obj.cnt-- == 1) {
      obj.a = 0
      return true
    }
    const ret = false
    return ret
  }
  imports.wbg.__wbindgen_error_new = function (arg0, arg1) {
    const ret = new Error(getStringFromWasm0(arg0, arg1))
    return addHeapObject(ret)
  }
  imports.wbg.__wbindgen_string_get = function (arg0, arg1) {
    const obj = getObject(arg1)
    const ret = typeof obj === 'string' ? obj : undefined
    var ptr1 = isLikeNone(ret)
      ? 0
      : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc)
    var len1 = WASM_VECTOR_LEN
    getInt32Memory0()[arg0 / 4 + 1] = len1
    getInt32Memory0()[arg0 / 4 + 0] = ptr1
  }
  imports.wbg.__wbg_fetch_25e3a297f7b04639 = function (arg0) {
    const ret = fetch(getObject(arg0))
    return addHeapObject(ret)
  }
  imports.wbg.__wbindgen_is_string = function (arg0) {
    const ret = typeof getObject(arg0) === 'string'
    return ret
  }
  imports.wbg.__wbindgen_is_falsy = function (arg0) {
    const ret = !getObject(arg0)
    return ret
  }
  imports.wbg.__wbg_new_abda76e883ba8a5f = function () {
    const ret = new Error()
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_stack_658279fe44541cf6 = function (arg0, arg1) {
    const ret = getObject(arg1).stack
    const ptr1 = passStringToWasm0(
      ret,
      wasm.__wbindgen_malloc,
      wasm.__wbindgen_realloc,
    )
    const len1 = WASM_VECTOR_LEN
    getInt32Memory0()[arg0 / 4 + 1] = len1
    getInt32Memory0()[arg0 / 4 + 0] = ptr1
  }
  imports.wbg.__wbg_error_f851667af71bcfc6 = function (arg0, arg1) {
    let deferred0_0
    let deferred0_1
    try {
      deferred0_0 = arg0
      deferred0_1 = arg1
      console.error(getStringFromWasm0(arg0, arg1))
    } finally {
      wasm.__wbindgen_free(deferred0_0, deferred0_1, 1)
    }
  }
  imports.wbg.__wbg_fetch_693453ca3f88c055 = function (arg0, arg1) {
    const ret = getObject(arg0).fetch(getObject(arg1))
    return addHeapObject(ret)
  }
  imports.wbg.__wbindgen_number_new = function (arg0) {
    const ret = arg0
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_instanceof_IdbFactory_32ca5b61b481d0d4 = function (arg0) {
    let result
    try {
      result = getObject(arg0) instanceof IDBFactory
    } catch (_) {
      result = false
    }
    const ret = result
    return ret
  }
  imports.wbg.__wbg_open_65e0826fa9c7929c = function () {
    return handleError(function (arg0, arg1, arg2, arg3) {
      const ret = getObject(arg0).open(
        getStringFromWasm0(arg1, arg2),
        arg3 >>> 0,
      )
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_open_7cc5c5a15ff86a65 = function () {
    return handleError(function (arg0, arg1, arg2) {
      const ret = getObject(arg0).open(getStringFromWasm0(arg1, arg2))
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_key_f5b181c13a2893c4 = function () {
    return handleError(function (arg0) {
      const ret = getObject(arg0).key
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_advance_4a9fc46f41566dd7 = function () {
    return handleError(function (arg0, arg1) {
      getObject(arg0).advance(arg1 >>> 0)
    }, arguments)
  }
  imports.wbg.__wbg_continue_ff6b09291a37ea4f = function () {
    return handleError(function (arg0) {
      getObject(arg0).continue()
    }, arguments)
  }
  imports.wbg.__wbg_target_52ddf6955f636bf5 = function (arg0) {
    const ret = getObject(arg0).target
    return isLikeNone(ret) ? 0 : addHeapObject(ret)
  }
  imports.wbg.__wbg_objectStoreNames_71c3096b04c76bcd = function (arg0) {
    const ret = getObject(arg0).objectStoreNames
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_createObjectStore_45c05e7be9907fde = function () {
    return handleError(function (arg0, arg1, arg2, arg3) {
      const ret = getObject(arg0).createObjectStore(
        getStringFromWasm0(arg1, arg2),
        getObject(arg3),
      )
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_deleteObjectStore_517effefcf018434 = function () {
    return handleError(function (arg0, arg1, arg2) {
      getObject(arg0).deleteObjectStore(getStringFromWasm0(arg1, arg2))
    }, arguments)
  }
  imports.wbg.__wbg_transaction_632c349fd48458fb = function () {
    return handleError(function (arg0, arg1, arg2) {
      const ret = getObject(arg0).transaction(getObject(arg1), takeObject(arg2))
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_new_7a20246daa6eec7e = function () {
    return handleError(function () {
      const ret = new Headers()
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_append_aa3f462f9e2b5ff2 = function () {
    return handleError(function (arg0, arg1, arg2, arg3, arg4) {
      getObject(arg0).append(
        getStringFromWasm0(arg1, arg2),
        getStringFromWasm0(arg3, arg4),
      )
    }, arguments)
  }
  imports.wbg.__wbg_value_818a84b71c8d6a92 = function () {
    return handleError(function (arg0) {
      const ret = getObject(arg0).value
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_newwithstrandinit_f581dff0d19a8b03 = function () {
    return handleError(function (arg0, arg1, arg2) {
      const ret = new Request(getStringFromWasm0(arg0, arg1), getObject(arg2))
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_indexNames_cd26d0c4a3e2e6d3 = function (arg0) {
    const ret = getObject(arg0).indexNames
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_createIndex_e1a9dfc378a45abb = function () {
    return handleError(function (arg0, arg1, arg2, arg3, arg4) {
      const ret = getObject(arg0).createIndex(
        getStringFromWasm0(arg1, arg2),
        getObject(arg3),
        getObject(arg4),
      )
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_delete_e8e3bfaf1ea215be = function () {
    return handleError(function (arg0, arg1) {
      const ret = getObject(arg0).delete(getObject(arg1))
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_deleteIndex_fdc764ebf52d4c6e = function () {
    return handleError(function (arg0, arg1, arg2) {
      getObject(arg0).deleteIndex(getStringFromWasm0(arg1, arg2))
    }, arguments)
  }
  imports.wbg.__wbg_openCursor_218846b7f56f5d54 = function () {
    return handleError(function (arg0) {
      const ret = getObject(arg0).openCursor()
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_openCursor_31878cfe72aac75c = function () {
    return handleError(function (arg0, arg1) {
      const ret = getObject(arg0).openCursor(getObject(arg1))
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_openCursor_028e15e1e8bc1d13 = function () {
    return handleError(function (arg0, arg1, arg2) {
      const ret = getObject(arg0).openCursor(getObject(arg1), takeObject(arg2))
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_put_23b163c3aeb63c96 = function () {
    return handleError(function (arg0, arg1) {
      const ret = getObject(arg0).put(getObject(arg1))
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_put_f50a8dd6e4a8a13a = function () {
    return handleError(function (arg0, arg1, arg2) {
      const ret = getObject(arg0).put(getObject(arg1), getObject(arg2))
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_setonupgradeneeded_73793bc200a4f7b8 = function (
    arg0,
    arg1,
  ) {
    getObject(arg0).onupgradeneeded = getObject(arg1)
  }
  imports.wbg.__wbg_result_915d75a0bb0397a1 = function () {
    return handleError(function (arg0) {
      const ret = getObject(arg0).result
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_error_a093a4b69c2260a3 = function () {
    return handleError(function (arg0) {
      const ret = getObject(arg0).error
      return isLikeNone(ret) ? 0 : addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_transaction_fe8e1f87ae7ea4cc = function (arg0) {
    const ret = getObject(arg0).transaction
    return isLikeNone(ret) ? 0 : addHeapObject(ret)
  }
  imports.wbg.__wbg_setonsuccess_a04d5d5a703ed886 = function (arg0, arg1) {
    getObject(arg0).onsuccess = getObject(arg1)
  }
  imports.wbg.__wbg_setonerror_80c9bac4e4864adf = function (arg0, arg1) {
    getObject(arg0).onerror = getObject(arg1)
  }
  imports.wbg.__wbg_setonabort_568145f0fa09b9be = function (arg0, arg1) {
    getObject(arg0).onabort = getObject(arg1)
  }
  imports.wbg.__wbg_setoncomplete_e9993a45b7bfaec4 = function (arg0, arg1) {
    getObject(arg0).oncomplete = getObject(arg1)
  }
  imports.wbg.__wbg_setonerror_d17408c3482b10eb = function (arg0, arg1) {
    getObject(arg0).onerror = getObject(arg1)
  }
  imports.wbg.__wbg_abort_7691b818613905b3 = function () {
    return handleError(function (arg0) {
      getObject(arg0).abort()
    }, arguments)
  }
  imports.wbg.__wbg_commit_07f92304c2c4ba17 = function () {
    return handleError(function (arg0) {
      getObject(arg0).commit()
    }, arguments)
  }
  imports.wbg.__wbg_objectStore_b0e52dee7e737df7 = function () {
    return handleError(function (arg0, arg1, arg2) {
      const ret = getObject(arg0).objectStore(getStringFromWasm0(arg1, arg2))
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_instanceof_Response_4c3b1446206114d1 = function (arg0) {
    let result
    try {
      result = getObject(arg0) instanceof Response
    } catch (_) {
      result = false
    }
    const ret = result
    return ret
  }
  imports.wbg.__wbg_url_83a6a4f65f7a2b38 = function (arg0, arg1) {
    const ret = getObject(arg1).url
    const ptr1 = passStringToWasm0(
      ret,
      wasm.__wbindgen_malloc,
      wasm.__wbindgen_realloc,
    )
    const len1 = WASM_VECTOR_LEN
    getInt32Memory0()[arg0 / 4 + 1] = len1
    getInt32Memory0()[arg0 / 4 + 0] = ptr1
  }
  imports.wbg.__wbg_status_d6d47ad2837621eb = function (arg0) {
    const ret = getObject(arg0).status
    return ret
  }
  imports.wbg.__wbg_headers_24def508a7518df9 = function (arg0) {
    const ret = getObject(arg0).headers
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_arrayBuffer_5b2688e3dd873fed = function () {
    return handleError(function (arg0) {
      const ret = getObject(arg0).arrayBuffer()
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_text_668782292b0bc561 = function () {
    return handleError(function (arg0) {
      const ret = getObject(arg0).text()
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_data_ba3ea616b5392abf = function (arg0) {
    const ret = getObject(arg0).data
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_length_acb2c4bcbfef2f1a = function (arg0) {
    const ret = getObject(arg0).length
    return ret
  }
  imports.wbg.__wbg_contains_f2be25be0242ccea = function (arg0, arg1, arg2) {
    const ret = getObject(arg0).contains(getStringFromWasm0(arg1, arg2))
    return ret
  }
  imports.wbg.__wbg_get_f31a9f341421cffd = function (arg0, arg1, arg2) {
    const ret = getObject(arg1)[arg2 >>> 0]
    var ptr1 = isLikeNone(ret)
      ? 0
      : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc)
    var len1 = WASM_VECTOR_LEN
    getInt32Memory0()[arg0 / 4 + 1] = len1
    getInt32Memory0()[arg0 / 4 + 0] = ptr1
  }
  imports.wbg.__wbg_addEventListener_9bf60ea8a362e5e4 = function () {
    return handleError(function (arg0, arg1, arg2, arg3) {
      getObject(arg0).addEventListener(
        getStringFromWasm0(arg1, arg2),
        getObject(arg3),
      )
    }, arguments)
  }
  imports.wbg.__wbg_addEventListener_374cbfd2bbc19ccf = function () {
    return handleError(function (arg0, arg1, arg2, arg3, arg4) {
      getObject(arg0).addEventListener(
        getStringFromWasm0(arg1, arg2),
        getObject(arg3),
        getObject(arg4),
      )
    }, arguments)
  }
  imports.wbg.__wbg_dispatchEvent_40c3472e9e4dcf5e = function () {
    return handleError(function (arg0, arg1) {
      const ret = getObject(arg0).dispatchEvent(getObject(arg1))
      return ret
    }, arguments)
  }
  imports.wbg.__wbg_removeEventListener_66ee1536a0b32c11 = function () {
    return handleError(function (arg0, arg1, arg2, arg3) {
      getObject(arg0).removeEventListener(
        getStringFromWasm0(arg1, arg2),
        getObject(arg3),
      )
    }, arguments)
  }
  imports.wbg.__wbg_signal_3c701f5f40a5f08d = function (arg0) {
    const ret = getObject(arg0).signal
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_new_0ae46f44b7485bb2 = function () {
    return handleError(function () {
      const ret = new AbortController()
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_abort_2c4fb490d878d2b2 = function (arg0) {
    getObject(arg0).abort()
  }
  imports.wbg.__wbg_wasClean_1efd9561c5671b45 = function (arg0) {
    const ret = getObject(arg0).wasClean
    return ret
  }
  imports.wbg.__wbg_code_72a380a2ce61a242 = function (arg0) {
    const ret = getObject(arg0).code
    return ret
  }
  imports.wbg.__wbg_reason_ad453a16ee68a1b9 = function (arg0, arg1) {
    const ret = getObject(arg1).reason
    const ptr1 = passStringToWasm0(
      ret,
      wasm.__wbindgen_malloc,
      wasm.__wbindgen_realloc,
    )
    const len1 = WASM_VECTOR_LEN
    getInt32Memory0()[arg0 / 4 + 1] = len1
    getInt32Memory0()[arg0 / 4 + 0] = ptr1
  }
  imports.wbg.__wbg_newwitheventinitdict_744eb6eb61245b7c = function () {
    return handleError(function (arg0, arg1, arg2) {
      const ret = new CloseEvent(
        getStringFromWasm0(arg0, arg1),
        getObject(arg2),
      )
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_readyState_c8f9a5deaec3bb41 = function (arg0) {
    const ret = getObject(arg0).readyState
    return ret
  }
  imports.wbg.__wbg_setbinaryType_68fc3c6feda7310c = function (arg0, arg1) {
    getObject(arg0).binaryType = takeObject(arg1)
  }
  imports.wbg.__wbg_new_2575c598b4006174 = function () {
    return handleError(function (arg0, arg1) {
      const ret = new WebSocket(getStringFromWasm0(arg0, arg1))
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_close_328b8b803521cbdd = function () {
    return handleError(function (arg0) {
      getObject(arg0).close()
    }, arguments)
  }
  imports.wbg.__wbg_send_5bf3f962e9ffe0f6 = function () {
    return handleError(function (arg0, arg1, arg2) {
      getObject(arg0).send(getStringFromWasm0(arg1, arg2))
    }, arguments)
  }
  imports.wbg.__wbg_send_2ba7d32fcb03b9a4 = function () {
    return handleError(function (arg0, arg1, arg2) {
      getObject(arg0).send(getArrayU8FromWasm0(arg1, arg2))
    }, arguments)
  }
  imports.wbg.__wbg_clearTimeout_541ac0980ffcef74 = function (arg0) {
    const ret = clearTimeout(takeObject(arg0))
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_setTimeout_7d81d052875b0f4f = function () {
    return handleError(function (arg0, arg1) {
      const ret = setTimeout(getObject(arg0), arg1)
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_queueMicrotask_3cbae2ec6b6cd3d6 = function (arg0) {
    const ret = getObject(arg0).queueMicrotask
    return addHeapObject(ret)
  }
  imports.wbg.__wbindgen_is_function = function (arg0) {
    const ret = typeof getObject(arg0) === 'function'
    return ret
  }
  imports.wbg.__wbg_queueMicrotask_481971b0d87f3dd4 = function (arg0) {
    queueMicrotask(getObject(arg0))
  }
  imports.wbg.__wbg_clearTimeout_76877dbc010e786d = function (arg0) {
    const ret = clearTimeout(takeObject(arg0))
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_setTimeout_75cb9b6991a4031d = function () {
    return handleError(function (arg0, arg1) {
      const ret = setTimeout(getObject(arg0), arg1)
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_crypto_1d1f22824a6a080c = function (arg0) {
    const ret = getObject(arg0).crypto
    return addHeapObject(ret)
  }
  imports.wbg.__wbindgen_is_object = function (arg0) {
    const val = getObject(arg0)
    const ret = typeof val === 'object' && val !== null
    return ret
  }
  imports.wbg.__wbg_process_4a72847cc503995b = function (arg0) {
    const ret = getObject(arg0).process
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_versions_f686565e586dd935 = function (arg0) {
    const ret = getObject(arg0).versions
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_node_104a2ff8d6ea03a2 = function (arg0) {
    const ret = getObject(arg0).node
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_require_cca90b1a94a0255b = function () {
    return handleError(function () {
      const ret = module.require
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_msCrypto_eb05e62b530a1508 = function (arg0) {
    const ret = getObject(arg0).msCrypto
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_randomFillSync_5c9c955aa56b6049 = function () {
    return handleError(function (arg0, arg1) {
      getObject(arg0).randomFillSync(takeObject(arg1))
    }, arguments)
  }
  imports.wbg.__wbg_getRandomValues_3aa56aa6edec874c = function () {
    return handleError(function (arg0, arg1) {
      getObject(arg0).getRandomValues(getObject(arg1))
    }, arguments)
  }
  imports.wbg.__wbg_new_16b304a2cfa7ff4a = function () {
    const ret = new Array()
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_newnoargs_e258087cd0daa0ea = function (arg0, arg1) {
    const ret = new Function(getStringFromWasm0(arg0, arg1))
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_next_40fc327bfc8770e6 = function (arg0) {
    const ret = getObject(arg0).next
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_next_196c84450b364254 = function () {
    return handleError(function (arg0) {
      const ret = getObject(arg0).next()
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_done_298b57d23c0fc80c = function (arg0) {
    const ret = getObject(arg0).done
    return ret
  }
  imports.wbg.__wbg_value_d93c65011f51a456 = function (arg0) {
    const ret = getObject(arg0).value
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_iterator_2cee6dadfd956dfa = function () {
    const ret = Symbol.iterator
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_get_e3c254076557e348 = function () {
    return handleError(function (arg0, arg1) {
      const ret = Reflect.get(getObject(arg0), getObject(arg1))
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_call_27c0f87801dedf93 = function () {
    return handleError(function (arg0, arg1) {
      const ret = getObject(arg0).call(getObject(arg1))
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_new_72fb9a18b5ae2624 = function () {
    const ret = new Object()
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_self_ce0dbfc45cf2f5be = function () {
    return handleError(function () {
      const ret = self.self
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_window_c6fb939a7f436783 = function () {
    return handleError(function () {
      const ret = window.window
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_globalThis_d1e6af4856ba331b = function () {
    return handleError(function () {
      const ret = globalThis.globalThis
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_global_207b558942527489 = function () {
    return handleError(function () {
      const ret = global.global
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbindgen_is_undefined = function (arg0) {
    const ret = getObject(arg0) === undefined
    return ret
  }
  imports.wbg.__wbg_push_a5b05aedc7234f9f = function (arg0, arg1) {
    const ret = getObject(arg0).push(getObject(arg1))
    return ret
  }
  imports.wbg.__wbg_instanceof_ArrayBuffer_836825be07d4c9d2 = function (arg0) {
    let result
    try {
      result = getObject(arg0) instanceof ArrayBuffer
    } catch (_) {
      result = false
    }
    const ret = result
    return ret
  }
  imports.wbg.__wbg_instanceof_Error_e20bb56fd5591a93 = function (arg0) {
    let result
    try {
      result = getObject(arg0) instanceof Error
    } catch (_) {
      result = false
    }
    const ret = result
    return ret
  }
  imports.wbg.__wbg_message_5bf28016c2b49cfb = function (arg0) {
    const ret = getObject(arg0).message
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_name_e7429f0dda6079e2 = function (arg0) {
    const ret = getObject(arg0).name
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_toString_ffe4c9ea3b3532e9 = function (arg0) {
    const ret = getObject(arg0).toString()
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_call_b3ca7c6051f9bec1 = function () {
    return handleError(function (arg0, arg1, arg2) {
      const ret = getObject(arg0).call(getObject(arg1), getObject(arg2))
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbg_getTime_2bc4375165f02d15 = function (arg0) {
    const ret = getObject(arg0).getTime()
    return ret
  }
  imports.wbg.__wbg_new0_7d84e5b2cd9fdc73 = function () {
    const ret = new Date()
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_new_81740750da40724f = function (arg0, arg1) {
    try {
      var state0 = { a: arg0, b: arg1 }
      var cb0 = (arg0, arg1) => {
        const a = state0.a
        state0.a = 0
        try {
          return __wbg_adapter_261(a, state0.b, arg0, arg1)
        } finally {
          state0.a = a
        }
      }
      const ret = new Promise(cb0)
      return addHeapObject(ret)
    } finally {
      state0.a = state0.b = 0
    }
  }
  imports.wbg.__wbg_resolve_b0083a7967828ec8 = function (arg0) {
    const ret = Promise.resolve(getObject(arg0))
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_then_0c86a60e8fcfe9f6 = function (arg0, arg1) {
    const ret = getObject(arg0).then(getObject(arg1))
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_then_a73caa9a87991566 = function (arg0, arg1, arg2) {
    const ret = getObject(arg0).then(getObject(arg1), getObject(arg2))
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_buffer_12d079cc21e14bdb = function (arg0) {
    const ret = getObject(arg0).buffer
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_newwithbyteoffsetandlength_aa4a17c33a06e5cb = function (
    arg0,
    arg1,
    arg2,
  ) {
    const ret = new Uint8Array(getObject(arg0), arg1 >>> 0, arg2 >>> 0)
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_new_63b92bc8671ed464 = function (arg0) {
    const ret = new Uint8Array(getObject(arg0))
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_set_a47bac70306a19a7 = function (arg0, arg1, arg2) {
    getObject(arg0).set(getObject(arg1), arg2 >>> 0)
  }
  imports.wbg.__wbg_length_c20a40f15020d68a = function (arg0) {
    const ret = getObject(arg0).length
    return ret
  }
  imports.wbg.__wbg_instanceof_Uint8Array_2b3bbecd033d19f6 = function (arg0) {
    let result
    try {
      result = getObject(arg0) instanceof Uint8Array
    } catch (_) {
      result = false
    }
    const ret = result
    return ret
  }
  imports.wbg.__wbg_newwithlength_e9b4878cebadb3d3 = function (arg0) {
    const ret = new Uint8Array(arg0 >>> 0)
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_subarray_a1f73cd4b5b42fe1 = function (arg0, arg1, arg2) {
    const ret = getObject(arg0).subarray(arg1 >>> 0, arg2 >>> 0)
    return addHeapObject(ret)
  }
  imports.wbg.__wbg_has_0af94d20077affa2 = function () {
    return handleError(function (arg0, arg1) {
      const ret = Reflect.has(getObject(arg0), getObject(arg1))
      return ret
    }, arguments)
  }
  imports.wbg.__wbg_set_1f9b04f170055d33 = function () {
    return handleError(function (arg0, arg1, arg2) {
      const ret = Reflect.set(getObject(arg0), getObject(arg1), getObject(arg2))
      return ret
    }, arguments)
  }
  imports.wbg.__wbg_stringify_8887fe74e1c50d81 = function () {
    return handleError(function (arg0) {
      const ret = JSON.stringify(getObject(arg0))
      return addHeapObject(ret)
    }, arguments)
  }
  imports.wbg.__wbindgen_debug_string = function (arg0, arg1) {
    const ret = debugString(getObject(arg1))
    const ptr1 = passStringToWasm0(
      ret,
      wasm.__wbindgen_malloc,
      wasm.__wbindgen_realloc,
    )
    const len1 = WASM_VECTOR_LEN
    getInt32Memory0()[arg0 / 4 + 1] = len1
    getInt32Memory0()[arg0 / 4 + 0] = ptr1
  }
  imports.wbg.__wbindgen_throw = function (arg0, arg1) {
    throw new Error(getStringFromWasm0(arg0, arg1))
  }
  imports.wbg.__wbindgen_memory = function () {
    const ret = wasm.memory
    return addHeapObject(ret)
  }
  imports.wbg.__wbindgen_closure_wrapper1470 = function (arg0, arg1, arg2) {
    const ret = makeMutClosure(arg0, arg1, 485, __wbg_adapter_32)
    return addHeapObject(ret)
  }
  imports.wbg.__wbindgen_closure_wrapper11865 = function (arg0, arg1, arg2) {
    const ret = makeMutClosure(arg0, arg1, 5994, __wbg_adapter_35)
    return addHeapObject(ret)
  }
  imports.wbg.__wbindgen_closure_wrapper11867 = function (arg0, arg1, arg2) {
    const ret = makeMutClosure(arg0, arg1, 5994, __wbg_adapter_35)
    return addHeapObject(ret)
  }
  imports.wbg.__wbindgen_closure_wrapper11869 = function (arg0, arg1, arg2) {
    const ret = makeMutClosure(arg0, arg1, 5994, __wbg_adapter_35)
    return addHeapObject(ret)
  }
  imports.wbg.__wbindgen_closure_wrapper11871 = function (arg0, arg1, arg2) {
    const ret = makeMutClosure(arg0, arg1, 5994, __wbg_adapter_42)
    return addHeapObject(ret)
  }
  imports.wbg.__wbindgen_closure_wrapper11927 = function (arg0, arg1, arg2) {
    const ret = makeMutClosure(arg0, arg1, 6019, __wbg_adapter_45)
    return addHeapObject(ret)
  }
  imports.wbg.__wbindgen_closure_wrapper13201 = function (arg0, arg1, arg2) {
    const ret = makeMutClosure(arg0, arg1, 6419, __wbg_adapter_48)
    return addHeapObject(ret)
  }
  imports.wbg.__wbindgen_closure_wrapper13916 = function (arg0, arg1, arg2) {
    const ret = makeMutClosure(arg0, arg1, 6604, __wbg_adapter_51)
    return addHeapObject(ret)
  }
  imports.wbg.__wbindgen_closure_wrapper13957 = function (arg0, arg1, arg2) {
    const ret = makeMutClosure(arg0, arg1, 6624, __wbg_adapter_54)
    return addHeapObject(ret)
  }

  return imports
}

function __wbg_init_memory(imports, maybe_memory) {}

function __wbg_finalize_init(instance, module) {
  wasm = instance.exports
  __wbg_init.__wbindgen_wasm_module = module
  cachedInt32Memory0 = null
  cachedUint8Memory0 = null

  return wasm
}

function initSync(module) {
  if (wasm !== undefined) return wasm

  const imports = __wbg_get_imports()

  __wbg_init_memory(imports)

  if (!(module instanceof WebAssembly.Module)) {
    module = new WebAssembly.Module(module)
  }

  const instance = new WebAssembly.Instance(module, imports)

  return __wbg_finalize_init(instance, module)
}

async function __wbg_init(input) {
  if (wasm !== undefined) return wasm

  if (typeof input === 'undefined') {
    input = new URL('fedimint_client_wasm_bg.wasm', import.meta.url)
  }
  const imports = __wbg_get_imports()

  if (
    typeof input === 'string' ||
    (typeof Request === 'function' && input instanceof Request) ||
    (typeof URL === 'function' && input instanceof URL)
  ) {
    input = fetch(input)
  }

  __wbg_init_memory(imports)

  const { instance, module } = await __wbg_load(await input, imports)

  return __wbg_finalize_init(instance, module)
}

export { initSync }
export default __wbg_init
