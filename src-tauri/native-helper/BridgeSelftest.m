// Intentionally one translation unit: exercise the real static policy functions.
#import "Bridge.m"
#include <limits.h>
#include <stdio.h>

static NSUInteger checks;
static void Assert(BOOL condition, NSString *message) {
    checks++;
    if (!condition) @throw [NSException exceptionWithName:@"BridgeSelftest" reason:message userInfo:nil];
}
static BOOL JSONBool(id value) {
    return value && CFGetTypeID((__bridge CFTypeRef)value) == CFBooleanGetTypeID();
}
static NSDictionary *Call(const char *operation) {
    char *response = pluxel_helper_call(operation, "null");
    Assert(response != NULL, @"Bridge returns allocated JSON");
    NSData *data = [NSData dataWithBytes:response length:strlen(response)];
    pluxel_helper_free(response);
    NSError *error = nil;
    id result = [NSJSONSerialization JSONObjectWithData:data options:0 error:&error];
    Assert(!error && [result isKindOfClass:NSDictionary.class], @"Bridge returns a JSON object");
    Assert(JSONBool(result[@"ok"]), @"Envelope ok is a JSON boolean");
    return result;
}
int main(void) { @autoreleasepool {
    if (getuid() == 0 || geteuid() == 0) { fprintf(stderr, "Run as a non-root user\n"); return 2; }
    int fd = -1;
    NSString *root = nil;
    int result = 1;
    @try {
        // Fail before any call that could reach ServiceManagement or LA.
        Assert(![NSBundle.mainBundle.bundlePath hasPrefix:@"/Applications/"], @"Selftest must remain outside /Applications");
        Assert(!PXAvailable(), @"Unbundled test executable is unavailable");
        NSDictionary *status = Call("status");
        Assert([status[@"ok"] boolValue], @"Status succeeds without registering a service");
        NSDictionary *value = status[@"value"];
        Assert([value isKindOfClass:NSDictionary.class], @"Status value is an object");
        for (NSString *key in @[@"supported", @"available", @"enabled", @"unlocked"])
            Assert(JSONBool(value[key]), [@"JSON boolean type: " stringByAppendingString:key]);
        Assert(![value[@"available"] boolValue] && ![value[@"enabled"] boolValue], @"Unbundled status unavailable and disabled");
        BOOL supported = NO;
        if (@available(macOS 13.0, *)) supported = YES;
        Assert([value[@"supported"] boolValue] == supported, @"Supported follows runtime availability");
        for (const char **op = (const char *[]){"unknown", "register", NULL}; *op; ++op) {
            Assert(!PXAvailable(), @"Guard before operation: no actual service registration or UI");
            NSDictionary *rejected = Call(*op);
            Assert(![rejected[@"ok"] boolValue] && [rejected[@"error"] isKindOfClass:NSString.class], @"Unbundled operation rejected");
        }
        Assert(PXAdminOwner(0), @"Root is a trusted owner");
        uuid_t user, admin;
        int member = 0;
        Assert(mbr_uid_to_uuid(getuid(), user) == 0 && mbr_gid_to_uuid(80, admin) == 0 &&
               mbr_check_membership(user, admin, &member) == 0, @"Resolve current admin membership");
        BOOL isAdmin = member != 0;
        Assert(PXAdminOwner(getuid()) == isAdmin, @"Owner policy matches membership, not UID range");
        char template[PATH_MAX];
        Assert(snprintf(template, sizeof(template), "%s/px-bridge-XXXXXX", NSTemporaryDirectory().fileSystemRepresentation) < (int)sizeof(template), @"Temporary path fits");
        Assert(mkdtemp(template) != NULL, @"Create private temporary fixture");
        root = [NSString stringWithUTF8String:template];
        NSString *file = [root stringByAppendingPathComponent:@"owned-file"];
        fd = open(file.fileSystemRepresentation, O_RDWR | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0600);
        Assert(fd >= 0, @"Create current-user-owned regular file");
        acl_t empty = acl_init(0);
        Assert(empty != NULL, @"Allocate empty ACL");
        int cleared = acl_set_fd_np(fd, empty, ACL_TYPE_EXTENDED); acl_free(empty);
        Assert(cleared == 0 && fchmod(fd, 0600) == 0, @"Clear inherited ACL and normalize mode");
        Assert(PXTrustedFD(fd) == isAdmin, @"Admin-owned file accepted; non-admin-owned file rejected");
        Assert(fchmod(fd, 0602) == 0 && !PXTrustedFD(fd), @"World-writable file rejected");
        Assert(fchmod(fd, 0600) == 0, @"Restore restrictive mode");
        acl_t acl = acl_init(1); acl_entry_t entry; acl_permset_t perms;
        Assert(acl != NULL && acl_create_entry(&acl, &entry) == 0, @"Allocate unsafe ACL entry");
        uuid_t staff;
        Assert(mbr_gid_to_uuid(20, staff) == 0 && acl_set_tag_type(entry, ACL_EXTENDED_ALLOW) == 0 &&
               acl_set_qualifier(entry, staff) == 0 && acl_get_permset(entry, &perms) == 0 &&
               acl_clear_perms(perms) == 0 && acl_add_perm(perms, ACL_WRITE_DATA) == 0 &&
               acl_set_permset(entry, perms) == 0, @"Build staff write ACL");
        int applied = acl_set_fd_np(fd, acl, ACL_TYPE_EXTENDED); acl_free(acl);
        Assert(applied == 0 && !PXTrustedFD(fd), @"Non-admin-group write ACL rejected");
        Assert(!PXTrustedFD(-1), @"Invalid FD rejected");
        for (NSString *path in @[@"/Applications-nontrust/Pluxel.app", @"/Applications", @"Applications/Pluxel.app", @"/Applications/../tmp/Pluxel.app", file])
            Assert(!PXTrustedPath(path, NO) && !PXTrustedPath(path, YES), @"Prefix/traversal/non-Applications path rejected");
        NSString *link = [root stringByAppendingPathComponent:@"linked-file"];
        Assert(symlink(file.fileSystemRepresentation, link.fileSystemRepresentation) == 0, @"Create temporary symlink");
        Assert(!PXTrustedPath(link, YES), @"Temporary symlink path rejected outside allowed prefix");
        // Exercise session transitions without presenting LocalAuthentication
        // or registering a service. Each selftest invocation is a fresh process.
        Assert(!PXIsUnlocked(), @"New app process begins locked");
        NSUInteger generation = PXAuthenticationGeneration();
        Assert(!PXCompleteAuthentication(generation, NO) && !PXIsUnlocked(), @"Failed authentication cannot unlock");
        Assert(PXCompleteAuthentication(generation, YES) && PXIsUnlocked(), @"Successful authentication unlocks the process session");
        __block NSUInteger submissions = 0;
        Assert(PXSubmitAuthorized(generation, ^{ ++submissions; }) && submissions == 1, @"Unlocked session can submit work");
        Assert([PXAuthenticate()[@"ok"] boolValue] && PXIsUnlocked(), @"An unlocked session skips the native authentication prompt");
        Assert([PXAuthenticate()[@"ok"] boolValue] && PXIsUnlocked(), @"Further operations share session approval");
        PXLockSession();
        Assert(!PXIsUnlocked(), @"Manual lock forgets session approval");
        Assert(!PXSubmitAuthorized(generation, ^{ ++submissions; }) && submissions == 1, @"Lock during handshake prevents unsent mutation");
        Assert(!PXCompleteAuthentication(generation, YES) && !PXIsUnlocked(), @"Late authentication cannot undo a manual lock");
        Assert(PXCompleteAuthentication(PXAuthenticationGeneration(), YES), @"Fresh authentication can unlock again");
        Assert(!PXSubmitAuthorized(generation, ^{ ++submissions; }) && submissions == 1, @"Old authorization cannot submit into a new session");
        Assert([Call("lock")[@"ok"] boolValue] && !PXIsUnlocked(), @"Rust lock operation clears the native session");
        printf("PASS: %lu bridge checks; current owner %s\n", (unsigned long)checks, isAdmin ? "admin (positive FD plus mode/ACL negatives)" : "non-admin (owner rejection; admin-positive FD case unavailable)");
        printf("NOTE: no /Applications writes; in-prefix symlink traversal and signed-bundle registration not exercised\n");
        result = 0;
    } @catch (NSException *failure) {
        fprintf(stderr, "FAIL after %lu checks: %s\n", (unsigned long)checks, failure.reason.UTF8String);
    } @finally {
        if (fd >= 0) close(fd);
        if (root) [NSFileManager.defaultManager removeItemAtPath:root error:nil];
    }
    return result;
}}
