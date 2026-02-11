/**
 * File System Operations Demo for Azul GUI Framework
 * 
 * This example demonstrates:
 * - Reading and writing files
 * - Creating directories
 * - Listing directory contents
 * - File metadata access
 * - Path manipulation
 * 
 * Compile with: 
 *   gcc -o file file.c -I. -L../../target/release -lazul -Wl,-rpath,../../target/release
 */

#include "azul.h"
#include <stdio.h>
#include <string.h>

// Helper to create AzString from C string
AzString az_str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

// Helper to create AzFilePath from C string
AzFilePath az_path(const char* s) {
    return AzFilePath_new(az_str(s));
}

// Helper struct for managing null-terminated C strings from AzString
typedef struct {
    AzU8Vec vec;
} CStr;

CStr cstr_new(const AzString* s) {
    CStr result;
    result.vec = AzString_toCStr(s);
    return result;
}

const char* cstr_ptr(const CStr* c) {
    return (const char*)c->vec.ptr;
}

void cstr_free(CStr* c) {
    AzU8Vec_delete(&c->vec);
}

#define WITH_CSTR(azstr, varname, code) do { \
    CStr _tmp_cstr = cstr_new(&(azstr)); \
    const char* varname = cstr_ptr(&_tmp_cstr); \
    code; \
    cstr_free(&_tmp_cstr); \
} while(0)

// ============================================================================
// Path Manipulation Demo
// ============================================================================

void demo_path_operations(void) {
    printf("\n============================================================\n");
    printf("Path Manipulation Demo\n");
    printf("============================================================\n\n");
    
    // Get temp directory
    AzFilePath temp = AzFilePath_getTempDir();
    AzString temp_str = AzFilePath_asString(&temp);
    WITH_CSTR(temp_str, temp_path, {
        printf("System temp directory: %s\n", temp_path);
    });
    AzString_delete(&temp_str);
    
    // Join paths
    printf("\nPath joining:\n");
    AzFilePath base = az_path("/home/user");
    AzFilePath joined = AzFilePath_joinStr(&base, az_str("documents/file.txt"));
    AzString joined_str = AzFilePath_asString(&joined);
    WITH_CSTR(joined_str, path, { printf("  /home/user + documents/file.txt = %s\n", path); });
    AzString_delete(&joined_str);
    AzFilePath_delete(&joined);
    AzFilePath_delete(&base);
    
    // Get parent directory
    printf("\nParent directory:\n");
    AzFilePath path = az_path("/home/user/documents/file.txt");
    AzOptionFilePath parent = AzFilePath_parent(&path);
    if (parent.Some.tag == AzOptionFilePath_Tag_Some) {
        AzString parent_str = AzFilePath_asString(&parent.Some.payload);
        WITH_CSTR(parent_str, p, { printf("  Parent of /home/user/documents/file.txt = %s\n", p); });
        AzString_delete(&parent_str);
        AzFilePath_delete(&parent.Some.payload);
    }
    AzFilePath_delete(&path);
    
    // Get filename
    printf("\nFilename extraction:\n");
    AzFilePath path2 = az_path("/home/user/documents/file.txt");
    AzOptionString filename = AzFilePath_fileName(&path2);
    if (filename.Some.tag == AzOptionString_Tag_Some) {
        WITH_CSTR(filename.Some.payload, f, { printf("  Filename of /home/user/documents/file.txt = %s\n", f); });
        AzString_delete(&filename.Some.payload);
    }
    AzFilePath_delete(&path2);
    
    // Get extension
    printf("\nExtension extraction:\n");
    AzFilePath path3 = az_path("/home/user/documents/file.txt");
    AzOptionString ext = AzFilePath_extension(&path3);
    if (ext.Some.tag == AzOptionString_Tag_Some) {
        WITH_CSTR(ext.Some.payload, e, { printf("  Extension of file.txt = %s\n", e); });
        AzString_delete(&ext.Some.payload);
    }
    AzFilePath_delete(&path3);
    
    // Check path types
    printf("\nPath type checking:\n");
    AzFilePath dir_path = az_path("/tmp");
    printf("  /tmp is file:      %s\n", AzFilePath_isFile(&dir_path) ? "true" : "false");
    printf("  /tmp is directory: %s\n", AzFilePath_isDir(&dir_path) ? "true" : "false");
    printf("  /tmp exists:       %s\n", AzFilePath_exists(&dir_path) ? "true" : "false");
    AzFilePath_delete(&dir_path);
    
    AzFilePath_delete(&temp);
}

// ============================================================================
// File Read/Write Demo
// ============================================================================

void demo_file_operations(void) {
    printf("\n============================================================\n");
    printf("File Read/Write Demo\n");
    printf("============================================================\n\n");
    
    // Create a test directory in temp
    AzFilePath temp = AzFilePath_getTempDir();
    AzFilePath test_dir = AzFilePath_joinStr(&temp, az_str("azul_file_demo"));
    
    AzString test_dir_str = AzFilePath_asString(&test_dir);
    WITH_CSTR(test_dir_str, dir_path, {
        printf("Creating test directory: %s\n", dir_path);
    });
    AzString_delete(&test_dir_str);
    
    // Create directory (will succeed or already exists)
    AzResultVoidFileError dir_result = AzFilePath_createDirAll(&test_dir);
    if (dir_result.Ok.tag == AzResultVoidFileError_Tag_Ok) {
        printf("  Directory created successfully\n");
    } else {
        printf("  Directory creation failed (may already exist)\n");
        AzFileError_delete(&dir_result.Err.payload);
    }
    
    // Write a text file
    printf("\nWriting text file...\n");
    AzFilePath file_path = AzFilePath_joinStr(&test_dir, az_str("test.txt"));
    
    const char* content = "Hello from Azul!\nThis is a test file.\nLine 3.";
    AzU8Vec data = AzU8Vec_copyFromBytes((const uint8_t*)content, 0, strlen(content));
    
    AzResultVoidFileError write_result = AzFilePath_writeBytes(&file_path, data);
    if (write_result.Ok.tag == AzResultVoidFileError_Tag_Ok) {
        printf("  Successfully wrote %zu bytes\n", strlen(content));
    } else {
        WITH_CSTR(write_result.Err.payload.message, err, {
            printf("  Write failed: %s\n", err);
        });
        AzFileError_delete(&write_result.Err.payload);
    }
    
    // Read the file back
    printf("\nReading file back...\n");
    AzResultU8VecFileError read_result = AzFilePath_readBytes(&file_path);
    if (read_result.Ok.tag == AzResultU8VecFileError_Tag_Ok) {
        AzU8Vec read_data = read_result.Ok.payload;
        printf("  Read %zu bytes:\n", read_data.len);
        printf("  ---\n");
        // Print content (assuming it's text)
        printf("  %.*s\n", (int)read_data.len, (char*)read_data.ptr);
        printf("  ---\n");
        AzU8Vec_delete(&read_data);
    } else {
        WITH_CSTR(read_result.Err.payload.message, err, {
            printf("  Read failed: %s\n", err);
        });
        AzFileError_delete(&read_result.Err.payload);
    }
    
    // Get file metadata
    printf("\nFile metadata:\n");
    AzResultFileMetadataFileError meta_result = AzFilePath_metadata(&file_path);
    if (meta_result.Ok.tag == AzResultFileMetadataFileError_Tag_Ok) {
        AzFileMetadata meta = meta_result.Ok.payload;
        printf("  Size:        %llu bytes\n", (unsigned long long)meta.size);
        printf("  Type:        ");
        switch (meta.file_type) {
            case AzFileType_File: printf("File\n"); break;
            case AzFileType_Directory: printf("Directory\n"); break;
            case AzFileType_Symlink: printf("Symlink\n"); break;
            default: printf("Other\n"); break;
        }
        printf("  Read-only:   %s\n", meta.is_readonly ? "true" : "false");
        printf("  Modified:    %llu (unix timestamp)\n", (unsigned long long)meta.modified_secs);
    } else {
        WITH_CSTR(meta_result.Err.payload.message, err, {
            printf("  Metadata failed: %s\n", err);
        });
        AzFileError_delete(&meta_result.Err.payload);
    }
    
    // Copy the file
    printf("\nCopying file...\n");
    AzFilePath copy_path = AzFilePath_joinStr(&test_dir, az_str("test_copy.txt"));
    
    AzResultu64FileError copy_result = AzFilePath_copyTo(&file_path, copy_path);
    if (copy_result.Ok.tag == AzResultu64FileError_Tag_Ok) {
        printf("  Copied %llu bytes\n", (unsigned long long)copy_result.Ok.payload);
    } else {
        WITH_CSTR(copy_result.Err.payload.message, err, {
            printf("  Copy failed: %s\n", err);
        });
        AzFileError_delete(&copy_result.Err.payload);
    }
    
    // Clean up
    AzFilePath_delete(&file_path);
    AzFilePath_delete(&test_dir);
    AzFilePath_delete(&temp);
}

// ============================================================================
// Directory Listing Demo
// ============================================================================

void demo_directory_listing(void) {
    printf("\n============================================================\n");
    printf("Directory Listing Demo\n");
    printf("============================================================\n\n");
    
    // Get temp directory
    AzFilePath temp = AzFilePath_getTempDir();
    AzFilePath test_dir = AzFilePath_joinStr(&temp, az_str("azul_file_demo"));
    
    // Create a few more files for demonstration
    for (int i = 1; i <= 3; i++) {
        char name[32];
        snprintf(name, sizeof(name), "file_%d.txt", i);
        AzFilePath fpath = AzFilePath_joinStr(&test_dir, az_str(name));
        
        char content[64];
        snprintf(content, sizeof(content), "Content of file %d", i);
        AzU8Vec file_data = AzU8Vec_copyFromBytes((const uint8_t*)content, 0, strlen(content));
        
        AzFilePath_writeBytes(&fpath, file_data);
        
        AzFilePath_delete(&fpath);
    }
    
    // List directory contents
    AzString test_dir_str = AzFilePath_asString(&test_dir);
    WITH_CSTR(test_dir_str, dir_path, {
        printf("Listing contents of: %s\n\n", dir_path);
    });
    AzString_delete(&test_dir_str);
    
    AzResultDirEntryVecFileError list_result = AzFilePath_readDir(&test_dir);
    if (list_result.Ok.tag == AzResultDirEntryVecFileError_Tag_Ok) {
        AzDirEntryVec entries = list_result.Ok.payload;
        
        printf("  %-30s %-10s\n", "Name", "Type");
        printf("  %-30s %-10s\n", "----", "----");
        
        for (size_t i = 0; i < entries.len; i++) {
            AzDirEntry* entry = &((AzDirEntry*)entries.ptr)[i];
            
            CStr name_cstr = cstr_new(&entry->name);
            const char* name = cstr_ptr(&name_cstr);
            
            const char* type_str;
            switch (entry->file_type) {
                case AzFileType_File: type_str = "File"; break;
                case AzFileType_Directory: type_str = "Dir"; break;
                case AzFileType_Symlink: type_str = "Link"; break;
                default: type_str = "Other"; break;
            }
            
            printf("  %-30s %-10s\n", name, type_str);
            
            cstr_free(&name_cstr);
        }
        
        printf("\n  Total: %zu entries\n", entries.len);
        
        AzDirEntryVec_delete(&entries);
    } else {
        WITH_CSTR(list_result.Err.payload.message, err, {
            printf("  Listing failed: %s\n", err);
        });
        AzFileError_delete(&list_result.Err.payload);
    }
    
    // Clean up - delete all files and directory
    printf("\nCleaning up test directory...\n");
    AzResultVoidFileError del_result = AzFilePath_removeDirAll(&test_dir);
    if (del_result.Ok.tag == AzResultVoidFileError_Tag_Ok) {
        printf("  Cleanup successful\n");
    } else {
        printf("  Cleanup failed (files may remain)\n");
        AzFileError_delete(&del_result.Err.payload);
    }
    
    AzFilePath_delete(&test_dir);
    AzFilePath_delete(&temp);
}

// ============================================================================
// Error Handling Demo
// ============================================================================

void demo_error_handling(void) {
    printf("\n============================================================\n");
    printf("Error Handling Demo\n");
    printf("============================================================\n\n");
    
    // Try to read a non-existent file
    printf("Attempting to read non-existent file...\n");
    AzFilePath bad_path = az_path("/this/path/does/not/exist/file.txt");
    AzResultU8VecFileError result = AzFilePath_readBytes(&bad_path);
    
    if (result.Err.tag == AzResultU8VecFileError_Tag_Err) {
        AzFileError err = result.Err.payload;
        
        printf("  Error kind: ");
        switch (err.kind) {
            case AzFileErrorKind_NotFound:
                printf("NotFound\n");
                break;
            case AzFileErrorKind_PermissionDenied:
                printf("PermissionDenied\n");
                break;
            case AzFileErrorKind_AlreadyExists:
                printf("AlreadyExists\n");
                break;
            case AzFileErrorKind_InvalidPath:
                printf("InvalidPath\n");
                break;
            case AzFileErrorKind_IoError:
                printf("IoError\n");
                break;
            case AzFileErrorKind_DirectoryNotEmpty:
                printf("DirectoryNotEmpty\n");
                break;
            case AzFileErrorKind_IsDirectory:
                printf("IsDirectory\n");
                break;
            case AzFileErrorKind_IsFile:
                printf("IsFile\n");
                break;
            default:
                printf("Other\n");
                break;
        }
        
        WITH_CSTR(err.message, msg, {
            printf("  Message: %s\n", msg);
        });
        
        AzFileError_delete(&err);
    }
    AzFilePath_delete(&bad_path);
    
    // Try to delete a non-empty directory (create first)
    printf("\nAttempting to delete non-empty directory...\n");
    AzFilePath temp = AzFilePath_getTempDir();
    AzFilePath test_dir = AzFilePath_joinStr(&temp, az_str("azul_error_demo"));
    AzFilePath_delete(&temp);
    
    AzFilePath_createDirAll(&test_dir);
    
    // Create a file inside
    AzFilePath fpath = AzFilePath_joinStr(&test_dir, az_str("file.txt"));
    AzU8Vec file_data = AzU8Vec_copyFromBytes((const uint8_t*)"test", 0, 4);
    AzFilePath_writeBytes(&fpath, file_data);
    AzFilePath_delete(&fpath);
    
    // Try to delete directory (not recursive)
    AzResultVoidFileError del_result = AzFilePath_removeDir(&test_dir);
    if (del_result.Err.tag == AzResultVoidFileError_Tag_Err) {
        printf("  Expected error: DirectoryNotEmpty\n");
        printf("  Got error kind: ");
        switch (del_result.Err.payload.kind) {
            case AzFileErrorKind_DirectoryNotEmpty:
                printf("DirectoryNotEmpty (correct!)\n");
                break;
            default:
                printf("Other (unexpected)\n");
                break;
        }
        AzFileError_delete(&del_result.Err.payload);
    }
    
    // Clean up with recursive delete
    AzFilePath_removeDirAll(&test_dir);
    AzFilePath_delete(&test_dir);
}

// ============================================================================
// Main
// ============================================================================

int main(void) {
    printf("Azul File System Operations Demo\n");
    printf("==================================\n");
    
    demo_path_operations();
    demo_file_operations();
    demo_directory_listing();
    demo_error_handling();
    
    printf("\n============================================================\n");
    printf("Demo complete!\n");
    printf("============================================================\n");
    
    return 0;
}
