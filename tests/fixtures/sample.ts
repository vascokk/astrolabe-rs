interface MyInterface {
    myMethod(): string;
}

class MyClass implements MyInterface {
    myMethod(): string {
        return "hello";
    }
}

function topLevelFunc(): number {
    return 42;
}
