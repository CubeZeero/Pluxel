#import "Protocol.h"
#import "Filesystem.h"
#include <unistd.h>

static dispatch_queue_t operations;

@interface PXPeer : NSObject <PXHelperProtocol>
@property uid_t uid;
@end

@implementation PXPeer
- (void)pingWithReply:(void (^)(NSData *))reply {
    reply(PXJSON(@{ @"protocol": @1 }));
}
- (void)applyRequest:(NSData *)request withReply:(void (^)(NSData *))reply {
    if (request.length > 1024 * 1024) { reply(PXJSON(PXError(@"Request too large"))); return; }
    uid_t uid = self.uid;
    dispatch_async(operations, ^{
        @autoreleasepool {
            @try {
                NSDictionary *payload = [NSJSONSerialization JSONObjectWithData:request options:0 error:nil];
                if (![payload isKindOfClass:NSDictionary.class]) reply(PXJSON(PXError(@"Invalid request")));
                else reply(PXJSON(PXApply(payload, uid)));
            } @catch (NSException *exception) { reply(PXJSON(PXError(exception.reason))); }
        }
    });
}
@end

@interface PXListener : NSObject <NSXPCListenerDelegate>
@end
@implementation PXListener
- (BOOL)listener:(NSXPCListener *)listener shouldAcceptNewConnection:(NSXPCConnection *)connection {
    (void)listener;
    if (connection.effectiveUserIdentifier < 501) return NO;
    // Kernel-enforced per-message verification, including PID-reuse protection.
    // No PID-only lookup and no private audit-token API.
    [connection setCodeSigningRequirement:PX_CLIENT_REQUIREMENT];
    PXPeer *peer = [PXPeer new];
    peer.uid = connection.effectiveUserIdentifier;
    connection.exportedInterface = [NSXPCInterface interfaceWithProtocol:@protocol(PXHelperProtocol)];
    connection.exportedObject = peer;
    [connection resume];
    return YES;
}
@end

int main(void) {
    @autoreleasepool {
        if (geteuid() != 0) return 77;
        if (@available(macOS 13.0, *)) {
            operations = dispatch_queue_create("com.cubezeero.pluxel.install-operations", DISPATCH_QUEUE_SERIAL);
            // NSXPCListener.delegate is weak. Keep both objects alive for the
            // entire daemon lifetime, including optimized release builds.
            static PXListener *delegate;
            static NSXPCListener *listener;
            delegate = [PXListener new];
            listener = [[NSXPCListener alloc] initWithMachServiceName:PX_SERVICE];
            [listener setConnectionCodeSigningRequirement:PX_CLIENT_REQUIREMENT];
            listener.delegate = delegate;
            [listener resume];
            [[NSRunLoop currentRunLoop] run];
        }
    }
    return 0;
}
