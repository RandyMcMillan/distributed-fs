import { loadPackageDefinition, credentials, Metadata } from "@grpc/grpc-js"
import fs from "fs"
import { loadSync } from "@grpc/proto-loader";
import path from "path";
import split from "split"

const PROTO_PATH = path.join(__dirname, '../proto/api.proto'),
	SERVER_URL = "192.168.0.164:50051"

const packageDefinition = loadSync(
	PROTO_PATH,
	{
		keepCase: true,
		longs: String,
		enums: String,
		defaults: true,
		oneofs: true,
	}
);

const apiProto = loadPackageDefinition(packageDefinition).api as any;

const client = new apiProto.Api(SERVER_URL, credentials.createInsecure());

const meta = new Metadata();
meta.add('public_key', '03efb57d7e730b13ca6b23822f4ecbeb7926a529e02befe4e4a8034c56ce607670');

const getEntry = (location: string) => {
	client.Get(
		{
			location
		},
		meta,
		(err: any, data: any) => {
			console.log(data, err)
		}
	)
}

const putEntry = () => {
	client.Put(
		{
			signature: "304402205cb2bf1b2619f84bf9e88f1b288ad47b982a569d6921d0f62d2548062b6fedb902207043d35fe41b6b272b12379ba04db1b2c68731b36f3f038182e4c6d8aad4850d",
			entry: {
				owner: "owner",
				public: true,
				read_users: [
					"User"
				],
				name: "Hello"
			}
		},
		meta,
		(err: any, data: any) => {
			console.log(data, err)
		}
	)

}

const uploadFile = () => {
	let call = client.Upload((err: any, res: any) => {
		console.log(err, res)
	})

	call.write({ metadata: { name: "testname", type: "testtype" } })

	const input = fs
		.createReadStream(path.join(__dirname, "../package-lock.json"));
	// .createReadStream(path.join(__dirname, "../test.txt"));

	input.pipe(split())
		// input
		.on("data", (chunk) => {
			call.write({ file: { content: Buffer.from(chunk, "utf-8") } })
			// call.write({ file: { content: chunk } })
		})
		.on("end", () =>
			call.end()
		)
}

uploadFile()
// putEntry()
// getEntry("e_304402205cb2bf1b2619f84bf9e88f1b288ad47b982a569d6921d0f62d2548062b6fedb902207043d35fe41b6b272b12379ba04db1b2c68731b36f3f038182e4c6d8aad4850d")
