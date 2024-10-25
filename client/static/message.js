// error case
// 1. server down
// -> ( reconnect the other server )
// 2. server still live but message was lost. this case occure when
//    pass the message to raft leader but the raft leader goes down. 
// -> ( retransmission )

export class MsgHandler{
    #msgQue = []
    #msgAge = []
    #sendIndexToServer = 0;
    #msgSize = 1;
    #msgLimit = 8;
    #ageLimit = 3;
    #timeStamp
    constructor(){
        this.#timeStamp = 0;
    }

    get getQue(){
        return [...this.#msgQue];
    }

    append(id, user_id, content){
        this.#timeStamp += 1;
        this.#msgQue.push(new Msg(id, user_id, content, this.#timeStamp));
        this.#msgAge.push(0);
        return this.#timeStamp;
    }

    aging(){
        for(let i=0;i<this.#msgAge.length;i++){
            this.#msgAge[i] += 1;
        }
        if(this.#msgAge[0]>=this.#ageLimit){
            console.log("msg age limit over");
            this.#sendIndexToServer = 0;
            this.#msgSize = 1;
        }
    }

    doubleMsgSize(){
        this.#msgSize *= 2;
        if(this.#msgSize > this.#msgLimit){
            this.#msgSize = this.#msgLimit;
        }
    }
   
    toJsonArray(){
        let temp = [];
        let i;
        for(i = this.#sendIndexToServer; i < this.#msgSize; i++){
            if(i >= this.#msgQue.length){
                break;
            }
            temp.push(this.#msgQue[i].toJson());
        }

        this.#sendIndexToServer = i;

        return temp;
    }

    queSize(){
        return this.#msgQue.length;
    }

    // cleanUp must works like pop front.
    cleanUp(timeStamp){
        for (let i = 0; i < this.#msgQue.length; i++) {
            if (this.#msgQue[i].timeStamp === timeStamp) {
                this.#msgQue.splice(i, 1); // 해당 인덱스의 msg 삭제
                this.#msgAge.splice(i, 1); // 해당 인덱스의 age 삭제
                this.#sendIndexToServer -= 1; // sendIndexToServer 감소
                if(this.#sendIndexToServer < 0){
                    this.#sendIndexToServer = 0;
                }
            }
        }
    }
}

export class Msg {
    #id;
    #userId;
    #content;
    #time;
    #timeStamp;

    constructor(id, userId, content, timeStamp){
        this.#id = id;
        this.#userId = userId;
        this.#content = content;
        this.#time = new Date().toISOString();
        this.#timeStamp = timeStamp;
    }

    // Getter
    get id() {
        return this.#id;
    }

    get userId() {
        return this.#userId;
    }

    get content() {
        return this.#content;
    }

    get time() {
        return this.#time;
    }

    get timeStamp() {
        return this.#timeStamp;
    }

    toJson(){
        return {
            id: this.#id,
            user_id: this.#userId,
            content: this.#content,
            time: this.#time,
            time_stamp: this.#timeStamp,
        };
    }

    isEqual(other) {
        if (!(other instanceof Msg)) return false; // 다른 타입일 경우 false
        return (
            this.id === other.id &&
            this.user_id === other.user_id &&
            this.content === other.content &&
            this.time === other.time &&
            this.timeStamp === other.timeStamp
        );
    }
}
