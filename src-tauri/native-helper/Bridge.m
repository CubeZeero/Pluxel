#import "Protocol.h"
#import <LocalAuthentication/LocalAuthentication.h>
#import <ServiceManagement/ServiceManagement.h>
#import <Security/Security.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/acl.h>
#include <membership.h>
#include <fcntl.h>
#include <unistd.h>
#include <errno.h>

static NSString *const PXRequestedKey = @"PluxelPrivilegedHelperRequested";

// Process-local authorization state only. No password, biometric data or token
// is persisted, and a new app process always starts locked.
static BOOL PXSessionUnlocked = NO;
static NSUInteger PXSessionGeneration = 0;
static LAContext *PXActiveAuthentication = nil;
static NSObject *PXSessionMutex(void) {
    static NSObject *mutex; static dispatch_once_t once;
    dispatch_once(&once, ^{ mutex = [NSObject new]; });
    return mutex;
}
static BOOL PXIsUnlocked(void) {
    @synchronized(PXSessionMutex()) { return PXSessionUnlocked; }
}
static NSUInteger PXAuthenticationGeneration(void) {
    @synchronized(PXSessionMutex()) { return PXSessionGeneration; }
}
static void PXLockSession(void) {
    LAContext *context;
    @synchronized(PXSessionMutex()) {
        PXSessionUnlocked = NO;
        ++PXSessionGeneration;
        context = PXActiveAuthentication;
        PXActiveAuthentication = nil;
    }
    [context invalidate];
}
static BOOL PXCompleteAuthentication(NSUInteger generation, BOOL success) {
    @synchronized(PXSessionMutex()) {
        if (!success || generation != PXSessionGeneration) return NO;
        PXSessionUnlocked = YES;
        return YES;
    }
}
static BOOL PXSubmitAuthorized(NSUInteger generation, void (^submit)(void)) {
    @synchronized(PXSessionMutex()) {
        if (!PXSessionUnlocked || generation != PXSessionGeneration) return NO;
        // Submission, not the filesystem operation, is serialized with Lock.
        // Already-submitted work may finish; unsent work cannot pass a lock.
        submit();
        return YES;
    }
}

// This is Pluxel's installation policy, not an extra SMAppService entitlement:
// trust root/admin-owned installations, including normal Finder installations,
// but never register code that a non-admin account can replace.
static BOOL PXAdminOwner(uid_t uid) {
    if (uid == 0) return YES;
    uuid_t user, admin;
    int member = 0;
    return mbr_uid_to_uuid(uid, user) == 0 && mbr_gid_to_uuid(80, admin) == 0 &&
        mbr_check_membership(user, admin, &member) == 0 && member;
}

static BOOL PXTrustedFD(int fd) {
    struct stat metadata;
    if (fstat(fd, &metadata) || !PXAdminOwner(metadata.st_uid) || (metadata.st_mode & 0002) ||
        ((metadata.st_mode & 0020) && metadata.st_gid != 80)) return NO;
    acl_t acl = acl_get_fd_np(fd, ACL_TYPE_EXTENDED);
    if (!acl) return errno == ENOENT || errno == ENOTSUP;
    BOOL valid = YES;
    acl_entry_t entry;
    int status = acl_get_entry(acl, ACL_FIRST_ENTRY, &entry);
    while (status == 0 && valid) {
        acl_tag_t tag;
        if (acl_get_tag_type(entry, &tag)) { valid = NO; break; }
        if (tag == ACL_EXTENDED_ALLOW) {
            acl_permset_t permissions;
            if (acl_get_permset(entry, &permissions)) { valid = NO; break; }
            acl_perm_t writes[] = { ACL_WRITE_DATA, ACL_APPEND_DATA, ACL_DELETE, ACL_DELETE_CHILD,
                ACL_WRITE_ATTRIBUTES, ACL_WRITE_EXTATTRIBUTES, ACL_WRITE_SECURITY, ACL_CHANGE_OWNER };
            BOOL canWrite = NO;
            for (NSUInteger i = 0; i < sizeof(writes) / sizeof(writes[0]); ++i) {
                int bit = acl_get_perm_np(permissions, writes[i]);
                if (bit < 0) { valid = NO; break; }
                if (bit) canWrite = YES;
            }
            if (valid && canWrite) {
                void *qualifier = acl_get_qualifier(entry);
                id_t identity = 0; int type = 0;
                if (!qualifier || mbr_uuid_to_id(qualifier, &identity, &type)) valid = NO;
                else valid = (type == ID_TYPE_UID && PXAdminOwner(identity)) || (type == ID_TYPE_GID && identity == 80);
                if (qualifier) acl_free(qualifier);
            }
        }
        status = acl_get_entry(acl, ACL_NEXT_ENTRY, &entry);
    }
    if (status != 1 && errno != EINVAL) valid = NO;
    acl_free(acl);
    return valid;
}

static BOOL PXTrustedPath(NSString *path, BOOL regularFile) {
    if (![path hasPrefix:@"/Applications/"]) return NO;
    NSArray<NSString *> *parts = path.pathComponents;
    int fd = open("/", O_RDONLY | O_DIRECTORY | O_CLOEXEC);
    if (fd < 0) return NO;
    BOOL trusted = PXTrustedFD(fd);
    for (NSUInteger i = 1; trusted && i < parts.count; ++i) {
        NSString *part = parts[i];
        if (!part.length || [part isEqual:@"."] || [part isEqual:@".."]) { trusted = NO; break; }
        BOOL file = regularFile && i + 1 == parts.count;
        int next = openat(fd, part.fileSystemRepresentation, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC | (file ? 0 : O_DIRECTORY));
        close(fd); fd = next;
        struct stat metadata;
        trusted = fd >= 0 && PXTrustedFD(fd) && fstat(fd, &metadata) == 0 &&
            (file ? S_ISREG(metadata.st_mode) : S_ISDIR(metadata.st_mode));
    }
    if (fd >= 0) close(fd);
    return trusted;
}

static BOOL PXValidSelf(void) {
    SecCodeRef code = NULL;
    SecRequirementRef requirement = NULL;
    OSStatus result = SecCodeCopySelf(kSecCSDefaultFlags, &code);
    if (result == errSecSuccess)
        result = SecRequirementCreateWithString((__bridge CFStringRef)PX_CLIENT_REQUIREMENT, kSecCSDefaultFlags, &requirement);
    if (result == errSecSuccess)
        result = SecCodeCheckValidity(code, kSecCSStrictValidate, requirement);
    if (requirement) CFRelease(requirement);
    if (code) CFRelease(code);
    return result == errSecSuccess;
}

static BOOL PXAvailable(void) {
    if (@available(macOS 13.0, *)) {
        NSString *bundle = NSBundle.mainBundle.bundlePath;
        NSString *helper = [bundle stringByAppendingPathComponent:@"Contents/MacOS/pluxel-helper"];
        NSString *plist = [bundle stringByAppendingPathComponent:[@"Contents/Library/LaunchDaemons" stringByAppendingPathComponent:PX_PLIST]];
        // Unbundled `tauri dev` and unsigned apps must never register daemons.
        return [bundle hasPrefix:@"/Applications/"] && [bundle hasSuffix:@".app"] &&
            PXTrustedPath(bundle, NO) && PXTrustedPath(helper, YES) && PXTrustedPath(plist, YES) &&
            [NSFileManager.defaultManager isExecutableFileAtPath:helper] &&
            [NSFileManager.defaultManager fileExistsAtPath:plist] && PXValidSelf();
    }
    return NO;
}

static NSDictionary *PXStatus(void) {
    if (@available(macOS 13.0, *)) {
        BOOL available = PXAvailable();
        SMAppServiceStatus status = available ? [SMAppService daemonServiceWithPlistName:PX_PLIST].status : SMAppServiceStatusNotFound;
        NSString *label = @"notRegistered";
        switch (status) {
            case SMAppServiceStatusEnabled: label = @"enabled"; break;
            case SMAppServiceStatusRequiresApproval: label = @"requiresApproval"; break;
            case SMAppServiceStatusNotFound:
                // A never-registered service also reports NotFound. Keep the
                // legacy mode usable until the user actually requests setup.
                label = [NSUserDefaults.standardUserDefaults boolForKey:PXRequestedKey] ? @"notFound" : @"notRegistered";
                break;
            default: break;
        }
        BOOL enabled = available && status == SMAppServiceStatusEnabled;
        if (!enabled) PXLockSession();
        return @{ @"supported": @YES, @"available": available ? @YES : @NO,
                  @"enabled": enabled ? @YES : @NO, @"unlocked": (enabled && PXIsUnlocked()) ? @YES : @NO, @"status": label };
    }
    return @{ @"supported": @NO, @"available": @NO, @"enabled": @NO, @"unlocked": @NO, @"status": @"unsupported" };
}

static NSDictionary *PXAuthenticate(void) {
    // Serialize concurrent callers so they share the first successful prompt.
    static NSObject *authenticationMutex; static dispatch_once_t once;
    dispatch_once(&once, ^{ authenticationMutex = [NSObject new]; });
    @synchronized(authenticationMutex) {
    if (PXIsUnlocked()) return PXSuccess(nil);
    NSUInteger generation = PXAuthenticationGeneration();
    LAContext *context = [LAContext new];
    @synchronized(PXSessionMutex()) {
        if (generation != PXSessionGeneration) return PXError(@"Authentication session was locked");
        PXActiveAuthentication = context;
    }
    NSError *error = nil;
    // The OS offers Touch ID where usable, with its own password fallback.
    // No credential is returned to or stored by Pluxel.
    if (![context canEvaluatePolicy:LAPolicyDeviceOwnerAuthentication error:&error]) {
        @synchronized(PXSessionMutex()) { if (PXActiveAuthentication == context) PXActiveAuthentication = nil; }
        return PXError(error.localizedDescription ?: @"Device authentication unavailable");
    }
    dispatch_semaphore_t done = dispatch_semaphore_create(0);
    __block NSDictionary *result = nil;
    [context evaluatePolicy:LAPolicyDeviceOwnerAuthentication
            localizedReason:@"Pluxel: install, update or remove After Effects files"
                      reply:^(BOOL success, NSError *failure) {
        result = success ? PXSuccess(nil) : PXError(failure.localizedDescription ?: @"Authentication cancelled");
        dispatch_semaphore_signal(done);
    }];
    BOOL timeout = dispatch_semaphore_wait(done, dispatch_time(DISPATCH_TIME_NOW, 180 * NSEC_PER_SEC)) != 0;
    @synchronized(PXSessionMutex()) { if (PXActiveAuthentication == context) PXActiveAuthentication = nil; }
    if (timeout) {
        [context invalidate];
        return PXError(@"Authentication timed out; no install request was sent");
    }
    [context invalidate];
    if ([result[@"ok"] boolValue] && !PXCompleteAuthentication(generation, YES))
        return PXError(@"Authentication session was locked; no install request was sent");
    return result;
    }
}

static NSDictionary *PXSend(NSData *request, NSUInteger generation) {
    if (@available(macOS 13.0, *)) {
        NSXPCConnection *connection = [[NSXPCConnection alloc] initWithMachServiceName:PX_SERVICE options:NSXPCConnectionPrivileged];
        [connection setCodeSigningRequirement:PX_HELPER_REQUIREMENT];
        connection.remoteObjectInterface = [NSXPCInterface interfaceWithProtocol:@protocol(PXHelperProtocol)];
        [connection resume];
        __block NSData *response = nil;
        __block NSError *failure = nil;
        id<PXHelperProtocol> proxy = [connection synchronousRemoteObjectProxyWithErrorHandler:^(NSError *error) { failure = error; }];
        // An explicit protocol handshake prevents using a stale/incompatible daemon.
        __block NSData *hello = nil;
        [proxy pingWithReply:^(NSData *data) { hello = data; }];
        NSDictionary *protocol = hello ? [NSJSONSerialization JSONObjectWithData:hello options:0 error:nil] : nil;
        if (failure || ![protocol[@"protocol"] isEqual:@1]) {
            [connection invalidate];
            return PXError(failure.localizedDescription ?: @"Helper protocol mismatch; remove and register the helper again");
        }
        dispatch_semaphore_t done = dispatch_semaphore_create(0);
        id<PXHelperProtocol> writer = [connection remoteObjectProxyWithErrorHandler:^(NSError *error) {
            failure = error;
            dispatch_semaphore_signal(done);
        }];
        if (!PXSubmitAuthorized(generation, ^{
            [writer applyRequest:request withReply:^(NSData *data) {
                response = data;
                dispatch_semaphore_signal(done);
            }];
        })) {
            [connection invalidate];
            return PXError(@"Authentication session was locked; no install request was sent");
        }
        dispatch_semaphore_wait(done, DISPATCH_TIME_FOREVER);
        // Do not time out an accepted mutation then race it with a rollback.
        [connection invalidate];
        if (failure) return PXError(failure.localizedDescription);
        NSDictionary *result = response ? [NSJSONSerialization JSONObjectWithData:response options:0 error:nil] : nil;
        return [result isKindOfClass:NSDictionary.class] ? result : PXError(@"Invalid helper response");
    }
    return PXError(@"macOS 13 or later is required");
}

// Called on Rust's blocking worker, never on the main/UI thread. The caller
// owns the returned malloc string and releases it with pluxel_helper_free.
char *pluxel_helper_call(const char *operation, const char *payload) {
    @autoreleasepool {
        @try {
            NSString *op = operation ? [NSString stringWithUTF8String:operation] : nil;
            NSDictionary *result = nil;
            if ([op isEqualToString:@"status"]) {
                result = PXSuccess(PXStatus());
            } else if ([op isEqualToString:@"lock"]) {
                PXLockSession();
                result = PXSuccess(nil);
            } else if (!PXAvailable()) {
                result = PXError(@"Use the signed Pluxel app in /Applications on macOS 13 or later");
            } else if (@available(macOS 13.0, *)) {
                SMAppService *service = [SMAppService daemonServiceWithPlistName:PX_PLIST];
                if ([op isEqualToString:@"register"]) {
                    PXLockSession();
                    [NSUserDefaults.standardUserDefaults setBool:YES forKey:PXRequestedKey];
                    NSError *error = nil;
                    BOOL registered = service.status == SMAppServiceStatusEnabled || [service registerAndReturnError:&error];
                    if (service.status == SMAppServiceStatusRequiresApproval) {
                        dispatch_async(dispatch_get_main_queue(), ^{ [SMAppService openSystemSettingsLoginItems]; });
                        result = PXSuccess(PXStatus());
                    } else {
                        result = registered ? PXSuccess(PXStatus()) : PXError(error.localizedDescription);
                    }
                } else if ([op isEqualToString:@"unregister"]) {
                    PXLockSession();
                    NSError *error = nil;
                    BOOL removed = service.status == SMAppServiceStatusNotRegistered || service.status == SMAppServiceStatusNotFound || [service unregisterAndReturnError:&error];
                    if (removed) [NSUserDefaults.standardUserDefaults setBool:NO forKey:PXRequestedKey];
                    result = removed ? PXSuccess(PXStatus()) : PXError(error.localizedDescription);
                } else if ([op isEqualToString:@"authenticate"]) {
                    result = PXAuthenticate();
                } else if ([op isEqualToString:@"apply"]) {
                    NSData *data = payload ? [[NSString stringWithUTF8String:payload] dataUsingEncoding:NSUTF8StringEncoding] : nil;
                    if (service.status != SMAppServiceStatusEnabled) result = PXError(@"The helper is not approved in System Settings");
                    else if (!data || data.length > 1024 * 1024) result = PXError(@"Invalid helper request size");
                    else {
                        result = PXAuthenticate();
                        if ([result[@"ok"] boolValue]) result = PXSend(data, PXAuthenticationGeneration());
                    }
                } else result = PXError(@"Unknown helper operation");
            }
            NSData *json = PXJSON(result ?: PXError(@"Unsupported helper operation"));
            NSString *string = [[NSString alloc] initWithData:json encoding:NSUTF8StringEncoding];
            return strdup(string.UTF8String ?: "{\"ok\":false,\"error\":\"Native helper error\"}");
        } @catch (NSException *exception) {
            NSData *json = PXJSON(PXError(exception.reason));
            return strdup([[NSString alloc] initWithData:json encoding:NSUTF8StringEncoding].UTF8String);
        }
    }
}

void pluxel_helper_free(char *value) { free(value); }
