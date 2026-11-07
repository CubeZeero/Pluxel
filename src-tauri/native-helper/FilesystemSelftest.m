#import "Filesystem.h"
#include <sys/stat.h>
#include <unistd.h>
#include <fcntl.h>
#include <sys/acl.h>
#include <membership.h>
#include <errno.h>

static int checks;
static void Assert(BOOL b, NSString *s) { checks++; if (!b) { NSLog(@"FAIL: %@",s); exit(1); } }
// These hooks deterministically exercise capture ordering with one uid. They do
// not substitute for a privileged helper/unprivileged peer integration test.
static void CleanupTests(NSString *root) {
    Assert(PXTestOwnerAllowed(0,501,NO), @"Root ancestor allowed by production policy");
    Assert(!PXTestOwnerAllowed(501,501,NO), @"Peer ancestor rejected by production policy");
    Assert(!PXTestOwnerAllowed(502,501,NO), @"Other-user ancestor rejected");
    Assert(PXTestOwnerAllowed(0,501,YES), @"Root selected tree allowed");
    Assert(PXTestOwnerAllowed(501,501,YES), @"Authenticated peer selected tree allowed");
    Assert(!PXTestOwnerAllowed(502,501,YES), @"Other-user selected tree rejected");
    NSFileManager *fm = NSFileManager.defaultManager;
    for (NSUInteger scenario=0; scenario<5; scenario++) {
        NSString *fixture = [root stringByAppendingPathComponent:[NSString stringWithFormat:@"cleanup-%lu",(unsigned long)scenario]];
        NSString *incoming = [fixture stringByAppendingPathComponent:@"incoming"];
        NSString *trash = [fixture stringByAppendingPathComponent:@"quarantine"];
        NSString *outside = [fixture stringByAppendingPathComponent:@"escaped"];
        NSString *tree = [incoming stringByAppendingPathComponent:@"tree"];
        NSString *child = [tree stringByAppendingPathComponent:@"child"];
        NSString *grandchild = [child stringByAppendingPathComponent:@"grandchild"];
        Assert([fm createDirectoryAtPath:grandchild withIntermediateDirectories:YES attributes:@{NSFilePosixPermissions:@0755} error:nil], @"Create synthetic cleanup tree");
        Assert([fm createDirectoryAtPath:trash withIntermediateDirectories:NO attributes:@{NSFilePosixPermissions:@0700} error:nil], @"Create synthetic private quarantine");
        Assert([@"preserve" writeToFile:[grandchild stringByAppendingPathComponent:@"keep"] atomically:NO encoding:NSUTF8StringEncoding error:nil], @"Create cleanup sentinel");
        int parent = open(incoming.fileSystemRepresentation,O_RDONLY|O_DIRECTORY);
        int stage = open(trash.fileSystemRepresentation,O_RDONLY|O_DIRECTORY);
        int retainedTree = open(tree.fileSystemRepresentation,O_RDONLY|O_DIRECTORY);
        int retainedChild = open(child.fileSystemRepresentation,O_RDONLY|O_DIRECTORY);
        Assert(parent>=0 && stage>=0 && retainedTree>=0 && retainedChild>=0, @"Retain attacker directory descriptors");
        NSString *preserved = scenario==1 ? child : scenario==2 ? grandchild : tree;
        struct stat before; Assert(lstat(preserved.fileSystemRepresentation,&before)==0, @"Record escaped directory identity");
        __block BOOL fired = NO;
        __block BOOL sawSealed = NO;
        PXTestSetCleanupHook(^(NSString *event, int fd, NSString *name) {
            if ([event isEqual:@"sealed"]) {
                struct stat sealed; Assert(fstat(fd,&sealed)==0 && sealed.st_uid==geteuid() && (sealed.st_mode&07777)==0700, @"Captured cleanup directory sealed before enumeration");
                acl_t acl = acl_get_fd_np(fd,ACL_TYPE_EXTENDED);
                if (acl) {
                    acl_entry_t entry; int status = acl_get_entry(acl,ACL_FIRST_ENTRY,&entry);
                    Assert(status==1 || (status==-1 && errno==EINVAL), @"Captured directory has no retained ACL grants");
                    acl_free(acl);
                } else {
                    Assert(errno==ENOENT || errno==ENOTSUP, @"Absent sealed ACL has no retained grants");
                }
                sawSealed=YES; return;
            }
            if (fired) return;
            struct stat captured;
            if (scenario==0 || scenario==4) {
                if (![event isEqual:@"before-capture"] || fd!=parent || ![name isEqual:@"tree"]) return;
                fired=YES;
                Assert(rename(tree.fileSystemRepresentation,outside.fileSystemRepresentation)==0, @"Move selected name away before capture");
                if (scenario==4) Assert(symlink(outside.fileSystemRepresentation,tree.fileSystemRepresentation)==0, @"Inject symlink after preflight");
            } else {
                if (![event isEqual:@"captured"] || fstatat(fd,name.fileSystemRepresentation,&captured,AT_SYMLINK_NOFOLLOW)!=0) return;
                struct stat target; Assert(fstat(scenario==2 ? retainedChild : retainedTree,&target)==0, @"Stat retained descriptor");
                if (captured.st_ino!=target.st_ino || captured.st_dev!=target.st_dev) return;
                fired=YES;
                if (scenario==3) @throw [NSException exceptionWithName:@"PXTestCleanup" reason:@"Stop after durable capture" userInfo:nil];
                Assert(renameat(scenario==2 ? retainedChild : retainedTree,scenario==2 ? "grandchild" : "child",AT_FDCWD,outside.fileSystemRepresentation)==0, @"Move descendant through retained fd before its capture");
            }
        });
        NSDictionary *result = PXTestRemove(parent,@"tree",stage,getuid());
        PXTestSetCleanupHook(nil);
        Assert(fired, @"Deterministic cleanup race hook executed");
        if (scenario==1 || scenario==2) Assert(sawSealed, @"Race fixture exercised quarantine sealing");
        Assert([result[@"ok"] boolValue] == (scenario<3), @"Cleanup succeeds or fails closed as scheduled");
        if (scenario==3) {
            NSString *journal = [NSString stringWithContentsOfFile:[trash stringByAppendingPathComponent:@"cleanup-journal.jsonl"] encoding:NSUTF8StringEncoding error:nil];
            NSString *line = [[journal componentsSeparatedByString:@"\n"] firstObject];
            NSDictionary *record = line ? [NSJSONSerialization JSONObjectWithData:[line dataUsingEncoding:NSUTF8StringEncoding] options:0 error:nil] : nil;
            Assert([record[@"sourceParent"] isEqual:incoming] && [record[@"sourceName"] isEqual:@"tree"], @"Recovery journal preserves original name and parent");
            NSString *captured = [trash stringByAppendingPathComponent:record[@"quarantine"] ?: @"missing"];
            struct stat after; Assert(lstat(captured.fileSystemRepresentation,&after)==0 && after.st_ino==before.st_ino && after.st_uid==before.st_uid && after.st_mode==before.st_mode, @"Failed capture retains original identity and permissions before sealing");
            Assert([fm fileExistsAtPath:[captured stringByAppendingPathComponent:@"child/grandchild/keep"]], @"Failed capture retains recoverable contents");
        } else {
            struct stat after; Assert(lstat(outside.fileSystemRepresentation,&after)==0 && after.st_ino==before.st_ino && after.st_uid==before.st_uid && after.st_mode==before.st_mode, @"Moved-away directory ownership and mode untouched");
            NSString *suffix = scenario==1 ? @"grandchild/keep" : scenario==2 ? @"keep" : @"child/grandchild/keep";
            Assert([[NSString stringWithContentsOfFile:[outside stringByAppendingPathComponent:suffix] encoding:NSUTF8StringEncoding error:nil] isEqual:@"preserve"], @"Moved-away contents never traversed or removed");
            if (scenario==4) Assert([fm fileExistsAtPath:[trash stringByAppendingPathComponent:@"cleanup-journal.jsonl"]], @"Unsafe captured object retains recovery journal");
        }
        close(retainedChild); close(retainedTree); close(stage); close(parent);
        Assert([fm removeItemAtPath:fixture error:nil], @"Remove only synthetic cleanup fixture");
    }
}
int main(void) { @autoreleasepool {
    Assert(getuid()>=501,@"Run selftest as a normal macOS user");
    NSFileManager *fm=NSFileManager.defaultManager;
    char tmp[PATH_MAX]; realpath(NSTemporaryDirectory().fileSystemRepresentation,tmp);
    NSString *root=[[NSString stringWithUTF8String:tmp] stringByAppendingPathComponent:[@"px-policy-" stringByAppendingString:NSUUID.UUID.UUIDString]];
    NSString *home=[root stringByAppendingPathComponent:@"home"], *apps=[root stringByAppendingPathComponent:@"Applications"];
    NSString *stage=[home stringByAppendingPathComponent:@"Library/Application Support/com.cubezeero.pluxel/Pluxel/.deployment-12345678-1234-1234-1234-123456789abc"];
    NSString *scripts=[apps stringByAppendingPathComponent:@"Adobe After Effects 2026/Scripts"];
    for (NSString *p in @[stage,scripts]) Assert([fm createDirectoryAtPath:p withIntermediateDirectories:YES attributes:@{NSFilePosixPermissions:@0755} error:nil],@"Create fixture");
    NSString *contents=[[scripts stringByDeletingLastPathComponent] stringByAppendingPathComponent:@"Adobe After Effects.app/Contents"];
    [fm createDirectoryAtPath:contents withIntermediateDirectories:YES attributes:@{NSFilePosixPermissions:@0755} error:nil];
    [@{@"CFBundleIdentifier":@"com.adobe.AfterEffects"} writeToFile:[contents stringByAppendingPathComponent:@"Info.plist"] atomically:NO];
    NSString *src=[stage stringByAppendingPathComponent:@"source-0"], *dst=[scripts stringByAppendingPathComponent:@"test.jsx"];
    [@"new" writeToFile:src atomically:NO encoding:NSUTF8StringEncoding error:nil]; chmod(src.fileSystemRepresentation,06755);
    NSDictionary *(^run)(NSArray *,NSArray *)=^NSDictionary *(NSArray *pairs,NSArray *removes) { NSDictionary *r=PXTestApply(@{@"pairs":pairs,@"removes":removes},getuid(),home,apps); return r; };
    NSDictionary *(^pair)(NSString *,NSString *)=^NSDictionary *(NSString *s,NSString *d) { return @{@"source":s,@"destination":d}; };
    BOOL (^ok)(NSDictionary *)=^BOOL(NSDictionary *r){ return [r[@"ok"] boolValue]; };
    Assert(ok(run(@[pair(src,dst)],@[])),@"Copy file");
    NSString *aeRoot=[scripts stringByDeletingLastPathComponent];
    [@"Finder icon" writeToFile:[aeRoot stringByAppendingPathComponent:@"Icon\r"] atomically:NO encoding:NSUTF8StringEncoding error:nil];
    Assert(ok(run(@[pair(src,dst)],@[])),@"Finder custom folder Icon CR metadata accepted");
    [@{@"CFBundleIdentifier":@"com.adobe.AfterEffects.application"} writeToFile:[contents stringByAppendingPathComponent:@"Info.plist"] atomically:NO];
    Assert(ok(run(@[pair(src,dst)],@[])),@"Current AE application identifier accepted");
    struct stat st; stat(dst.fileSystemRepresentation,&st); Assert((st.st_mode&07777)==0755,@"Strip suid/sgid, preserve exec");
    Assert(ok(run(@[pair(src,dst)],@[dst])),@"Exact remove/copy replacement");
    [@"original" writeToFile:dst atomically:NO encoding:NSUTF8StringEncoding error:nil];
    PXTestFailAfterPublish(2);
    Assert(!ok(run(@[pair(src,dst)],@[dst])),@"Injected failure triggers rollback");
    PXTestFailAfterPublish(-1);
    Assert([[NSString stringWithContentsOfFile:dst encoding:NSUTF8StringEncoding error:nil] isEqual:@"original"],@"Rollback restores exact remove/copy original");
    Assert(!ok(run(@[pair(src,dst),pair(src,[scripts stringByAppendingPathComponent:@"TEST.jsx"])],@[])),@"Case alias copies rejected");
    NSArray *badDest=@[scripts,[scripts stringByAppendingPathComponent:@"ScriptUI Panels"],[scripts stringByAppendingPathComponent:@".metadata"],[scripts stringByAppendingPathComponent:@"../escape"],[root stringByAppendingPathComponent:@"outside"],[scripts stringByAppendingPathComponent:@"bad\nname"],[scripts stringByAppendingString:@"/test.jsx/"]];
    for (NSString *bad in badDest) Assert(!ok(run(@[pair(src,bad)],@[])),@"Reject destination");
    Assert(!ok(run(@[pair(src,dst),pair(src,dst)],@[])),@"Duplicate copy");
    Assert(!ok(run(@[],@[dst,dst])),@"Duplicate removal");
    Assert(!ok(run(@[pair(src,[dst stringByAppendingPathComponent:@"child"])],@[dst])),@"Ancestor overlap");
    NSString *second=[scripts stringByAppendingPathComponent:@"untouched.jsx"];
    Assert(!ok(run(@[pair(src,second),pair(@"/etc/passwd",dst)],@[])),@"Bad source rejects batch");
    Assert(![fm fileExistsAtPath:second],@"Preflight causes no writes");
    NSString *link=[stage stringByAppendingPathComponent:@"source-1"];
    symlink(src.fileSystemRepresentation,link.fileSystemRepresentation);
    Assert(!ok(run(@[pair(link,dst)],@[])),@"Source symlink rejected"); unlink(link.fileSystemRepresentation);
    linkat(AT_FDCWD,src.fileSystemRepresentation,AT_FDCWD,link.fileSystemRepresentation,0);
    Assert(!ok(run(@[pair(link,dst)],@[])),@"Source hardlink rejected"); unlink(link.fileSystemRepresentation);
    mkfifo(link.fileSystemRepresentation,0600);
    Assert(!ok(run(@[pair(link,dst)],@[])),@"FIFO rejected without blocking"); unlink(link.fileSystemRepresentation);
    int large=open(link.fileSystemRepresentation,O_CREAT|O_WRONLY|O_EXCL,0600);
    Assert(large>=0 && ftruncate(large,128*1024*1024+1)==0,@"Create sparse oversized source"); close(large);
    Assert(!ok(run(@[pair(link,dst)],@[])),@"Oversized source rejected before reads"); unlink(link.fileSystemRepresentation);
    NSString *moved=[stage stringByAppendingString:@"-moved"];
    Assert(rename(stage.fileSystemRepresentation,moved.fileSystemRepresentation)==0,@"Move deployment root");
    symlink(moved.fileSystemRepresentation,stage.fileSystemRepresentation);
    Assert(!ok(run(@[pair(src,dst)],@[])),@"Swapped deployment ancestor symlink rejected");
    unlink(stage.fileSystemRepresentation); rename(moved.fileSystemRepresentation,stage.fileSystemRepresentation);
    NSString *tree=[stage stringByAppendingPathComponent:@"source-2"];
    [fm createDirectoryAtPath:tree withIntermediateDirectories:NO attributes:nil error:nil];
    [@"nested" writeToFile:[tree stringByAppendingPathComponent:@"file"] atomically:NO encoding:NSUTF8StringEncoding error:nil];
    NSString *out=[scripts stringByAppendingPathComponent:@"ScriptUI Panels/Tool"];
    Assert(ok(run(@[pair(tree,out)],@[])),@"Recursive copy under protected container");
    // A previous manual installation can leave a peer-owned companion tree.
    Assert(ok(run(@[pair(tree,out)],@[out])),@"Replace existing peer-owned companion tree");
    [@"previous" writeToFile:[out stringByAppendingPathComponent:@"file"] atomically:NO encoding:NSUTF8StringEncoding error:nil];
    PXTestFailAfterPublish(2);
    Assert(!ok(run(@[pair(tree,out)],@[out])),@"Peer-owned tree publication failure rolls back");
    PXTestFailAfterPublish(-1);
    Assert([[NSString stringWithContentsOfFile:[out stringByAppendingPathComponent:@"file"] encoding:NSUTF8StringEncoding error:nil] isEqual:@"previous"],@"Peer-owned companion contents restored");
    Assert(ok(run(@[pair(tree,out)],@[out])),@"Peer-owned tree can be replaced after rollback");
    symlink("/etc",[[tree stringByAppendingPathComponent:@"escape"] fileSystemRepresentation]);
    Assert(!ok(run(@[pair(tree,out)],@[])),@"Nested source symlink rejected");
    Assert([[NSString stringWithContentsOfFile:[out stringByAppendingPathComponent:@"file"] encoding:NSUTF8StringEncoding error:nil] isEqual:@"nested"],@"Rejected tree preserves destination");
    NSString *evil=[scripts stringByAppendingPathComponent:@"linked"];
    symlink(root.fileSystemRepresentation,evil.fileSystemRepresentation);
    Assert(!ok(run(@[pair(src,[evil stringByAppendingPathComponent:@"escape"])],@[])),@"Destination ancestor symlink rejected");
    Assert(!ok(run(@[],@[evil])),@"Destination symlink rejected in preflight");
    NSString *unsafe=[scripts stringByAppendingPathComponent:@"unsafe"];
    [fm createDirectoryAtPath:unsafe withIntermediateDirectories:NO attributes:@{NSFilePosixPermissions:@0777} error:nil]; chmod(unsafe.fileSystemRepresentation,0777);
    Assert(!ok(run(@[pair(src,[unsafe stringByAppendingPathComponent:@"file"])],@[])),@"Writable destination ancestor rejected");
    Assert(!ok(PXTestApply(@{@"pairs":@[],@"removes":@[]},0,home,apps)),@"Root peer rejected");
    Assert(!ok(run((id)@"bad",@[])),@"Wrong payload type rejected");
    NSMutableArray *many=[NSMutableArray new]; for(int i=0;i<513;i++) [many addObject:dst];
    Assert(!ok(run(@[],many)),@"Item limit");
    NSString *fake=[apps stringByAppendingPathComponent:@"Adobe After Effects Fake/Scripts"];
    [fm createDirectoryAtPath:fake withIntermediateDirectories:YES attributes:@{NSFilePosixPermissions:@0755} error:nil];
    Assert(!ok(run(@[pair(src,[fake stringByAppendingPathComponent:@"test.jsx"])],@[])),@"Fake AE folder rejected");
    uuid_t userUUID; mbr_uid_to_uuid(getuid(),userUUID);
    acl_t acl=acl_init(1); acl_entry_t entry; acl_create_entry(&acl,&entry);
    acl_set_tag_type(entry,ACL_EXTENDED_ALLOW); acl_set_qualifier(entry,userUUID);
    acl_permset_t perms; acl_get_permset(entry,&perms); acl_add_perm(perms,ACL_WRITE_DATA); acl_set_permset(entry,perms);
    Assert(acl_set_file(scripts.fileSystemRepresentation,ACL_TYPE_EXTENDED,acl)==0,@"Install test ACL"); acl_free(acl);
    Assert(!ok(run(@[pair(src,dst)],@[])),@"Untrusted ACL write grant rejected");
    acl_t empty=acl_init(0); Assert(acl_set_file(scripts.fileSystemRepresentation,ACL_TYPE_EXTENDED,empty)==0,@"Clear test ACL"); acl_free(empty);
    NSString *stale=[apps stringByAppendingPathComponent:@".pluxel-helper-stale"];
    [fm createDirectoryAtPath:stale withIntermediateDirectories:NO attributes:nil error:nil];
    NSDictionary *blocked=run(@[],@[dst]);
    Assert(!ok(blocked) && [blocked[@"error"] containsString:stale],@"Stale stage blocks new requests with recovery path");
    Assert([fm fileExistsAtPath:dst],@"Stale stage does not mutate destination");
    [fm removeItemAtPath:stale error:nil];
    Assert(ok(run(@[],@[out,dst])),@"Recursive uninstall");
    Assert(![fm fileExistsAtPath:out] && ![fm fileExistsAtPath:dst],@"Uninstall completed");
    NSString *emptyCompanion = [scripts stringByAppendingPathComponent:@"ScriptUI Panels/BeatPad_lib"];
    Assert([fm createDirectoryAtPath:emptyCompanion withIntermediateDirectories:YES attributes:@{NSFilePosixPermissions:@0755} error:nil], @"Create synthetic empty peer-owned BeatPad_lib");
    Assert(ok(run(@[],@[emptyCompanion])) && ![fm fileExistsAtPath:emptyCompanion], @"Remove empty peer-owned companion without changing ancestors");
    CleanupTests(root);
    [fm removeItemAtPath:root error:nil];
    NSLog(@"PASS: %d filesystem policy checks",checks);
} return 0; }
