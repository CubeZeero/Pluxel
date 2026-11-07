#import <Foundation/Foundation.h>

// Keep the daemon label, Mach service and signature identities stable across
// updates. This is the public Developer ID team, not a signing secret.
#define PX_SERVICE @"com.cubezeero.pluxel.install-helper"
#define PX_PLIST @"com.cubezeero.pluxel.install-helper.plist"
#define PX_TEAM @"BVC35LQRJR"
#define PX_CLIENT_REQUIREMENT @"anchor apple generic and identifier \"com.cubezeero.pluxel\" and certificate leaf[subject.OU] = \"BVC35LQRJR\""
#define PX_HELPER_REQUIREMENT @"anchor apple generic and identifier \"com.cubezeero.pluxel.install-helper\" and certificate leaf[subject.OU] = \"BVC35LQRJR\""

@protocol PXHelperProtocol
- (void)pingWithReply:(void (^)(NSData *))reply;
- (void)applyRequest:(NSData *)request withReply:(void (^)(NSData *))reply;
@end

static inline NSData *PXJSON(NSDictionary *value) {
    return [NSJSONSerialization dataWithJSONObject:value options:0 error:nil];
}
static inline NSDictionary *PXError(NSString *message) {
    return @{ @"ok": @NO, @"error": message ?: @"Helper operation failed" };
}
static inline NSDictionary *PXSuccess(id value) {
    return @{ @"ok": @YES, @"value": value ?: [NSNull null] };
}
