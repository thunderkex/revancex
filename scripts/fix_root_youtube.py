import zipfile, struct, os, sys, shutil, hashlib, zlib, subprocess

def update_dex_hashes(dex_bytes: bytearray):
    sha1 = hashlib.sha1(dex_bytes[0x20:]).digest()
    dex_bytes[0x0C:0x20] = sha1
    adler = zlib.adler32(dex_bytes[0x0C:]) & 0xFFFFFFFF
    dex_bytes[0x08:0x0C] = struct.pack('<I', adler)

def patch_apk(apk_path: str, keystore_path: str, keystore_pass: str, key_alias: str):
    print(f"Patching {apk_path}...")
    
    target_str = b'Failed to fetch kids onboarding status, finishing the App.'
    
    modified_dexes = {}
    with zipfile.ZipFile(apk_path, 'r') as zin:
        for info in zin.infolist():
            if info.filename.endswith('.dex'):
                dex_data = bytearray(zin.read(info.filename))
                if target_str in dex_data:
                    print(f"  Found target in {info.filename}")
                    
                    # Find string index
                    string_ids_size = struct.unpack_from('<I', dex_data, 0x38)[0]
                    string_ids_off = struct.unpack_from('<I', dex_data, 0x3C)[0]
                    target_str_idx = -1
                    for i in range(string_ids_size):
                        str_off = struct.unpack_from('<I', dex_data, string_ids_off + i * 4)[0]
                        pos = str_off
                        while dex_data[pos] & 0x80: pos += 1
                        pos += 1
                        if dex_data[pos:pos+len(target_str)] == target_str:
                            target_str_idx = i
                            break
                    
                    if target_str_idx == -1:
                        print(f"  String index not found in {info.filename}")
                        continue
                    
                    print(f"  String index: {target_str_idx}")
                    idx_bytes_2 = struct.pack('<H', target_str_idx)
                    
                    # Find instruction offsets for const-string referencing this
                    hits = []
                    for i in range(len(dex_data) - 4):
                        if dex_data[i] == 0x1A and dex_data[i+2:i+4] == idx_bytes_2:
                            hits.append(i)
                    
                    print(f"  Found {len(hits)} occurrences in {info.filename}")
                    
                    # For each occurrence, find any invoke-virtual for finish() within next 40 bytes and NOP it
                    # invoke-virtual is 0x6e. Usually: 6e 10 <method_idx 2B> <reg> 00 (6 bytes)
                    # Also look for 0e 00 (return-void) right after
                    count_patched = 0
                    for hit in hits:
                        # Scan forward up to 250 bytes for `invoke-virtual` (0x6E) followed by return-void (0x0E 0x00)
                        window = dex_data[hit:hit+250]
                        ret_pos = window.find(b'\x0e\x00')
                        while ret_pos != -1:
                            if ret_pos >= 6 and window[ret_pos-6] == 0x6E:
                                abs_invoke = hit + ret_pos - 6
                                print(f"    NOPing invoke-virtual at offset {abs_invoke}: {dex_data[abs_invoke:abs_invoke+6].hex()}")
                                dex_data[abs_invoke:abs_invoke+6] = b'\x00\x00\x00\x00\x00\x00'
                                count_patched += 1
                            if ret_pos >= 10 and window[ret_pos-10] == 0x54:
                                abs_iget = hit + ret_pos - 10
                                print(f"    NOPing iget-object at offset {abs_iget}: {dex_data[abs_iget:abs_iget+4].hex()}")
                                dex_data[abs_iget:abs_iget+4] = b'\x00\x00\x00\x00'
                            
                            ret_pos = window.find(b'\x0e\x00', ret_pos + 2)
                    
                    if count_patched > 0:
                        update_dex_hashes(dex_data)
                        modified_dexes[info.filename] = bytes(dex_data)
                        print(f"  Successfully patched {count_patched} calls in {info.filename}")

    zipalign_cmd = "zipalign.exe" if os.name == 'nt' else "zipalign"
    if not modified_dexes:
        check = subprocess.run([zipalign_cmd, "-c", "-v", "4", apk_path], capture_output=True, text=True, shell=(os.name == 'nt'))
        if check.returncode == 0:
            print("No modifications needed and alignment verified.")
            return
        print("No DEX modifications needed, but page alignment required.")

    # Create new unaligned APK without signature files
    unaligned_apk = apk_path + ".unaligned.apk"
    with zipfile.ZipFile(apk_path, 'r') as zin, zipfile.ZipFile(unaligned_apk, 'w') as zout:
        for item in zin.infolist():
            if item.filename.startswith('META-INF/') and (item.filename.endswith('.RSA') or item.filename.endswith('.SF') or item.filename.endswith('.MF') or item.filename.endswith('.EC')):
                continue # Strip signature
            # Preserve compression type (especially resources.arsc which must be STORED)
            compress = item.compress_type
            if item.filename == 'resources.arsc' or item.filename.endswith('.so'):
                compress = zipfile.ZIP_STORED
            if item.filename in modified_dexes:
                zout.writestr(item.filename, modified_dexes[item.filename], compress_type=compress)
            else:
                zout.writestr(item.filename, zin.read(item.filename), compress_type=compress)

    # zipalign
    print(f"Aligning {apk_path} with zipalign...")
    zipalign_cmd = "zipalign.exe" if os.name == 'nt' else "zipalign"
    aligned_apk = apk_path + ".aligned.apk"
    align_res = subprocess.run([zipalign_cmd, "-p", "-f", "4", unaligned_apk, aligned_apk], capture_output=True, text=True, shell=(os.name == 'nt'))
    if align_res.returncode != 0:
        print("zipalign error:", align_res.stderr)
        sys.exit(1)

    os.replace(aligned_apk, apk_path)
    if os.path.exists(unaligned_apk):
        os.remove(unaligned_apk)
    
    # Re-sign with apksigner
    print(f"Signing {apk_path} with apksigner...")
    apksigner_cmd = "apksigner.bat" if os.name == 'nt' else "apksigner"
    sign_cmd = [
        apksigner_cmd, "sign",
        "--ks", keystore_path,
        "--ks-key-alias", key_alias,
        "--ks-pass", f"pass:{keystore_pass}",
        "--key-pass", f"pass:{keystore_pass}",
        apk_path
    ]
    res = subprocess.run(sign_cmd, capture_output=True, text=True, shell=(os.name == 'nt'))
    if res.returncode != 0:
        print("apksigner error:", res.stderr)
        sys.exit(1)
    print(f"Successfully re-signed {apk_path}")

if __name__ == '__main__':
    if len(sys.argv) >= 4:
        apk = sys.argv[1]
        ks = sys.argv[2]
        alias = sys.argv[3]
        pw = sys.argv[4] if len(sys.argv) > 4 else 'revanced'
        patch_apk(apk, ks, alias, pw)
    else:
        patch_apk('output/youtube-root.apk', 'keys/youtube.jks', 'revanced', 'revanced')
        patch_apk('output/youtube_music-root.apk', 'keys/youtube_music.jks', 'revanced', 'revanced')
